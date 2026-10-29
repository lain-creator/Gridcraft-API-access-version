//! Explicit selection/workbook translation. Jobs own immutable snapshots and results open as
//! separate unsaved workbooks; opening a file alone never sends any text to the service.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;

use crate::{Notice, NoticeKind, SheetApp};
use gridcraft_engine::{core::RangeRef, model::Workbook};
use serde_json::{Value as Json, json};

pub type Progress = Arc<Mutex<String>>;
#[derive(Default)]
pub struct Translation {
    pub bytes: Vec<u8>,
    pub changed: usize,
    pub skipped_bilingual: usize,
    pub reviewed: usize,
    pub skipped_other: usize,
    pub details: Vec<CellDecision>,
}
#[derive(Clone, serde::Serialize)]
pub struct CellDecision {
    pub sheet: String,
    pub cell: String,
    pub action: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    Workbook,
    Selection { sheet: usize, ranges: Vec<RangeRef> },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetLanguage {
    English,
    Chinese,
}
impl TargetLanguage {
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "英文",
            Self::Chinese => "中文",
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }
    pub fn api(self) -> (&'static str, &'static str) {
        match self {
            Self::English => ("zh-to-target", "English"),
            Self::Chinese => ("auto-to-zh", "Chinese"),
        }
    }
}
#[derive(Clone)]
pub struct Request {
    pub profile: Option<crate::ai::Profile>,
    pub wb: Arc<Workbook>,
    pub scope: Scope,
    pub target: TargetLanguage,
}
pub type Translator = Arc<dyn Fn(Request, Progress, &AtomicBool) -> Result<Translation, String> + Send + Sync>;

struct Snapshot {
    request: Request,
    name: String,
    uid: u64,
    label: String,
}
struct Job {
    snapshot: Snapshot,
    cancel: Arc<AtomicBool>,
    progress: Progress,
    worker: Option<JoinHandle<Result<Translation, String>>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) struct State {
    job: Option<Job>,
    visible: bool,
    status: Json,
    feedback: String,
    details: Vec<CellDecision>,
    whole_workbook: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            job: None,
            visible: false,
            status: json!({"state":"idle","pending":false}),
            feedback: String::new(),
            details: Vec::new(),
            whole_workbook: false,
        }
    }
}
fn output_name(name: &str, target: TargetLanguage) -> String {
    let stem = std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("Book");
    format!("{}.translated.{}.xlsx", stem.trim_end_matches(".translated.en").trim_end_matches(".translated.zh"), target.code())
}

impl SheetApp {
    fn translation_snapshot(&self, scope: &str, target: TargetLanguage) -> Result<Snapshot, String> {
        let doc = self.session.active().ok_or("请先打开一个工作簿。")?;
        if doc.wb.vba_project.is_some() {
            return Err("暂不翻译含 VBA 的工作簿，请先另存不含宏的 XLSX 副本。".into());
        }
        let (scope, label) = match scope {
            "workbook" => (Scope::Workbook, "整个工作簿（所有工作表）".to_string()),
            "selection" => {
                let sheet = doc.wb.sheet(doc.sheet_index()).ok_or("没有当前工作表。")?;
                (
                    Scope::Selection { sheet: doc.sheet_index(), ranges: doc.selection.ranges.clone() },
                    format!("选中区域：{}!{}", sheet.name, doc.selection.a1()),
                )
            }
            _ => return Err("scope 必须为 selection 或 workbook。".into()),
        };
        Ok(Snapshot {
            request: Request { wb: doc.wb.clone(), scope, target, profile: self.ai.settings.selected().ok().cloned() },
            name: doc.display_title(),
            uid: doc.uid,
            label,
        })
    }

    pub(crate) fn translation_command(&mut self, id: &str, p: &Json) -> Option<Result<Json, String>> {
        Some(match id {
            "ui.translateSelection" | "ui.translateWorkbook" | "ui.translateEnglish" | "ui.translateChinese" | "ui.pcbtool" => {
                self.translation.visible = true;
                if self.translation.job.is_some() {
                    return Some(Ok(self.translation.status.clone()));
                }
                if !self.commit_edit(0, 0, false, false) {
                    return Some(Err("请先完成当前单元格编辑。".into()));
                }
                let target = if id == "ui.translateChinese" { TargetLanguage::Chinese } else { TargetLanguage::English };
                let scope = match id {
                    "ui.translateEnglish" | "ui.translateChinese" => p.get("scope").and_then(Json::as_str).unwrap_or("selection"),
                    "ui.translateSelection" => "selection",
                    _ => "workbook",
                };
                // The explicit button is the user's request to send eligible text.
                self.translation_snapshot(scope, target).and_then(|s| self.start_translation(s))
            }
            "file.translateEnglish" | "file.translateChinese" => {
                if p.get("allowApiUpload").and_then(Json::as_bool) != Some(true) {
                    return Some(Err("需要 allowApiUpload:true，确认将所选正文发送到当前配置的第三方 API。".into()));
                }
                let target = if id == "file.translateChinese" { TargetLanguage::Chinese } else { TargetLanguage::English };
                self.translation_snapshot(
                    p.get("scope").and_then(Json::as_str).unwrap_or(if target == TargetLanguage::Chinese { "selection" } else { "workbook" }),
                    target,
                )
                .and_then(|s| self.start_translation(s))
            }
            "file.translateEnglish.status" | "file.translateChinese.status" => {
                let mut status = self.translation.status.clone();
                if let Some(job) = &self.translation.job {
                    status["message"] = json!(job.progress.lock().map(|s| s.clone()).unwrap_or_default());
                }
                Ok(status)
            }
            "file.translateEnglish.cancel" | "file.translateChinese.cancel" => {
                if let Some(job) = &self.translation.job {
                    job.cancel.store(true, Ordering::Relaxed);
                }
                Ok(json!({"cancelRequested":true}))
            }
            "options.pcbtool" => {
                if p.get("mode").and_then(Json::as_str).is_some_and(|m| m != "off" && m != "manual") {
                    Err("已改为按钮触发：请点击“开始”功能区右侧的翻译按钮。打开文件不自动翻译。".into())
                } else {
                    Ok(json!({"mode":"manual","skipBilingual":true}))
                }
            }
            _ => return None,
        })
    }

    fn start_translation(&mut self, snapshot: Snapshot) -> Result<Json, String> {
        if self.translation.job.is_some() {
            return Err("已有一个翻译任务，请完成或取消后再试。".into());
        }
        let service = self.services.translate_workbook.clone().ok_or("本平台未配置 API 翻译服务。")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let progress = Arc::new(Mutex::new("正在准备 AI 语义检查…".into()));
        let worker_progress = progress.clone();
        let request = snapshot.request.clone();
        let worker = std::thread::Builder::new()
            .name("api-translation".into())
            .spawn(move || service(request, worker_progress, &worker_cancel))
            .map_err(|e| e.to_string())?;
        self.translation.status = json!({"state":"running","pending":true,"source":snapshot.name,"sourceUid":snapshot.uid,"scope":snapshot.label,"targetLanguage":snapshot.request.target.code()});
        self.translation.job = Some(Job { snapshot, cancel, progress, worker: Some(worker) });
        self.translation.feedback.clear();
        self.translation.details.clear();
        self.translation.visible = true;
        Ok(self.translation.status.clone())
    }

    pub(crate) fn poll_translation(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.translation.job else { return };
        if !job.worker.as_ref().is_some_and(|w| w.is_finished()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        let Some(mut job) = self.translation.job.take() else { return };
        let cancelled = job.cancel.load(Ordering::Relaxed);
        let result = job
            .worker
            .take()
            .ok_or_else(|| "翻译任务丢失。".to_string())
            .and_then(|w| w.join().map_err(|_| "翻译线程异常；原工作簿未更改。".to_string()))
            .and_then(|r| r);
        let result = if cancelled { Err("已取消本机等待；已发送的服务端任务可能仍在运行。".into()) } else { result };
        let result = result.and_then(|r| {
            if r.changed == 0 {
                return Ok((r, None));
            }
            let name = output_name(&job.snapshot.name, job.snapshot.request.target);
            let opened = self.run("file.open", json!({"name":name,"base64":gridcraft_engine::io::base64_encode(&r.bytes)}))?;
            self.after_open(None, &opened);
            if let Some(doc) = self.session.active_mut() {
                doc.saved = job.snapshot.request.wb.clone();
            }
            Ok((r, Some(name)))
        });
        match result {
            Ok((r, name)) => {
                self.translation.feedback = if r.changed == 0 {
                    format!(
                        "AI 已检查 {} 个单元格；{} 个目标语言内容已完整，其他无需翻译 {} 个。没有新增译文，可展开查看处理明细。",
                        r.reviewed, r.skipped_bilingual, r.skipped_other
                    )
                } else {
                    format!(
                        "AI 已检查 {} 个单元格，补充译文 {} 个，目标语言内容已完整 {} 个，其他无需翻译 {} 个。结果已另开副本，请核对后另存。",
                        r.reviewed, r.changed, r.skipped_bilingual, r.skipped_other
                    )
                };
                self.translation.status = json!({"state":if r.changed == 0 {"skipped"} else {"succeeded"},"pending":false,"changedCells":r.changed,"skippedBilingual":r.skipped_bilingual,"reviewedCells":r.reviewed,"skippedOther":r.skipped_other,"details":r.details,"name":name,"scope":job.snapshot.label,"targetLanguage":job.snapshot.request.target.code(),"sourceUid":job.snapshot.uid});
                self.translation.details = r.details;
                self.notice = Some(Notice { text: self.translation.feedback.clone(), kind: NoticeKind::Info, action: None });
            }
            Err(e) => {
                self.translation.feedback = e.clone();
                self.translation.status = json!({"state":if cancelled {"cancelled"} else {"failed"},"pending":false,"error":e});
            }
        }
        self.translation.visible = true;
        self.after_engine();
        ctx.request_repaint();
    }
    pub fn shutdown_translation(&mut self) {
        if let Some(mut job) = self.translation.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
            if let Some(worker) = job.worker.take() {
                let _ = worker.join();
            }
        }
    }
    pub(crate) fn translation_controls(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 3.0;
        egui::ComboBox::from_id_salt("translation_scope")
            .width(88.0)
            .selected_text(if self.translation.whole_workbook { "整个工作簿" } else { "当前选区" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.translation.whole_workbook, false, "当前选区");
                ui.selectable_value(&mut self.translation.whole_workbook, true, "整个工作簿");
            });
        for (label, command) in [("中译英", "ui.translateEnglish"), ("英译中", "ui.translateChinese")] {
            if ui
                .add_enabled(self.session.active().is_some(), egui::Button::new(label).min_size(egui::vec2(100.0, 22.0)))
                .on_hover_text("仅点击方向按钮才发送所选范围；AI 检查双语完整性，优先 PCB 专业术语")
                .clicked()
            {
                let scope = if self.translation.whole_workbook { "workbook" } else { "selection" };
                self.run_or_alert(command, json!({"scope":scope}));
            }
        }
    }
    pub(crate) fn translation_options(&mut self, ui: &mut egui::Ui) {
        if self.services.translate_workbook.is_none() {
            return;
        }
        ui.strong("第三方 API 中英翻译");
        ui.label("“开始”工具栏右侧：先选择当前选区或整个工作簿，再点击“中译英”或“英译中”。打开文件或切换范围不发送内容。");
        ui.label(crate::i18n::current(ui.ctx()).tr("由 AI 判断是否已有完整译文；PCB/PTH/NPTH、型号和单位不再当作完整英文。结果保留原文，译文继承该单元格的字体和字号，自动换行并按需增高行。"));
        ui.label(
            crate::i18n::current(ui.ctx())
                .tr("所选范围内的候选正文发送到 API 设置中选择的服务商；不发送图片、公式或选区外单元格。可在结果中查看处理原因。"),
        );
    }
    pub(crate) fn translation_window(&mut self, ctx: &egui::Context) {
        if !self.translation.visible {
            return;
        }
        let mut visible = true;
        egui::Window::new("API · 中英翻译")
            .id(egui::Id::new("translation_result_v2"))
            .open(&mut visible)
            .default_size(egui::vec2(960.0, 640.0).min(ctx.content_rect().size() - egui::vec2(40.0, 70.0)))
            .min_size(egui::vec2(360.0, 260.0).min(ctx.content_rect().size() * 0.7))
            .constrain_to(ctx.content_rect())
            .resizable(true)
            .collapsible(true)
            .vscroll(true)
            .show(ctx, |ui| {
                if let Ok(profile) = self.ai.settings.selected() {
                    ui.label(format!("服务商：{} · {}", profile.name, profile.model));
                }
                if ui.button("API 设置").clicked() {
                    self.ai.settings_open = true;
                }
                if let Some(job) = &self.translation.job {
                    ui.label(format!("工作簿：{}", job.snapshot.name));
                    ui.label(format!("{} → {}", job.snapshot.label, job.snapshot.request.target.label()));
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(job.progress.lock().map(|s| s.clone()).unwrap_or_default());
                    });
                    ui.label(crate::i18n::current(ui.ctx()).tr("AI 正在判断完整双语与待译内容；型号和缩写不会直接跳过。取消停止本机等待。"));
                    if ui.button("取消翻译").clicked() {
                        self.run_or_alert("file.translateEnglish.cancel", json!({}));
                    }
                } else {
                    ui.label(&self.translation.feedback);
                    if !self.translation.details.is_empty() {
                        ui.collapsing("查看单元格处理明细", |ui| {
                            egui::ScrollArea::vertical().max_height(ui.available_height().max(280.0)).show(ui, |ui| {
                                for d in &self.translation.details {
                                    ui.add(egui::Label::new(format!("{}!{} · {}：{}", d.sheet, d.cell, d.action, d.reason)).wrap());
                                }
                            });
                        });
                    }
                    if ui.button("关闭").clicked() {
                        self.translation.visible = false;
                    }
                }
            });
        if !visible {
            self.translation.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    #[test]
    fn large_translation_window_is_resizable_and_fits_small_screens() {
        use egui_kittest::kittest::Queryable;
        for width in [1440.0, 800.0] {
            let mut app = fixture(Arc::new(|_, _, _| Err("No API calls expected".into()))).unwrap();
            app.translation.visible = true;
            app.translation.feedback = "合成界面检查：原文和译文保持独立，未调用 API。".into();
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(width, 900.0)).build_ui_state(
                |ui, app: &mut SheetApp| {
                    let ctx = ui.ctx().clone();
                    app.logic(&ctx);
                    app.ui(ui);
                },
                app,
            );
            h.run_steps(8);
            let rect = h.get_by_label("API · 中英翻译").rect();
            assert!(rect.width() <= width && rect.height() <= 900.0, "{rect:?}");
            if width > 1000.0 {
                assert!(rect.width() >= 900.0, "{rect:?}");
                if let Ok(dir) = std::env::var("CRAFT_UI_QA_DIR") {
                    h.render().unwrap().save(std::path::Path::new(&dir).join("gridcraft-translation.png")).unwrap();
                }
            }
            assert!(h.state().translation.job.is_none());
        }
    }
    fn fixture(service: Translator) -> Result<SheetApp, String> {
        let mut session = gridcraft_engine::Session::new();
        session.new_workbook();
        session.run("cell.set", json!({"cell":"A1","input":"板材"})).map_err(|e| e.to_string())?;
        Ok(SheetApp::new(session, Services { translate_workbook: Some(service), ..Default::default() }))
    }
    fn wait(app: &mut SheetApp) -> Result<(), String> {
        let ctx = egui::Context::default();
        for _ in 0..1000 {
            app.poll_translation(&ctx);
            if app.translation.job.is_none() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        app.shutdown_translation();
        Err("test worker timeout".into())
    }
    #[test]
    fn direction_buttons_send_only_the_chosen_scope_and_never_on_scope_change() -> Result<(), String> {
        use egui_kittest::kittest::Queryable;
        for whole in [false, true] {
            for (button, target) in [("中译英", TargetLanguage::English), ("英译中", TargetLanguage::Chinese)] {
                let calls = Arc::new(Mutex::new(Vec::new()));
                let worker_calls = calls.clone();
                let mut app = fixture(Arc::new(move |request, _, _| {
                    worker_calls.lock().map_err(|e| e.to_string())?.push((request.target, request.scope));
                    Ok(Translation::default())
                }))?;
                app.run("selection.set", json!({"range":"A1:B2"}))?;
                app.translation.whole_workbook = whole;
                let mut h = egui_kittest::Harness::builder()
                    .with_size(egui::vec2(800.0, 600.0))
                    .build_ui_state(|ui, app: &mut SheetApp| app.translation_controls(ui), app);
                h.run_steps(3);
                assert!(calls.lock().map_err(|e| e.to_string())?.is_empty());
                h.get_by_label(button).click();
                h.run_steps(3);
                wait(h.state_mut())?;
                let scope =
                    if whole { Scope::Workbook } else { Scope::Selection { sheet: 0, ranges: vec![RangeRef::parse("A1:B2").ok_or("range")?] } };
                assert_eq!(*calls.lock().map_err(|e| e.to_string())?, vec![(target, scope)]);
                assert_eq!(h.state().session.active().ok_or("doc")?.selection.a1(), "A1:B2");
            }
        }
        Ok(())
    }
    #[test]
    fn selected_snapshot_result_preserves_later_edits_and_selection() -> Result<(), String> {
        let mut app = fixture(Arc::new(|request, _, _| {
            assert_eq!(request.scope, Scope::Selection { sheet: 0, ranges: vec![RangeRef::parse("A1:B2").ok_or("range")?] });
            let bytes = gridcraft_engine::io::save_bytes(&request.wb, "a.xlsx").map_err(|e| e.to_string())?;
            Ok(Translation { bytes, changed: 1, ..Default::default() })
        }))?;
        let uid = app.session.active().ok_or("doc")?.uid;
        app.run("selection.set", json!({"range":"A1:B2"}))?;
        app.run("ui.translateSelection", json!({}))?;
        app.run("cell.set", json!({"cell":"A1","input":"later edit"}))?;
        app.run("selection.set", json!({"range":"D4"}))?;
        wait(&mut app)?;
        assert_eq!(app.translation.status["state"], "succeeded");
        assert_eq!(app.session.documents().len(), 2);
        let cell = gridcraft_engine::core::CellRef::new(0, 0);
        let original = app.session.documents().iter().find(|d| d.uid == uid).ok_or("source lost")?;
        assert_eq!(original.wb.sheets[0].cells.get(cell).ok_or("cell")?.value, gridcraft_engine::core::Value::Text("later edit".into()));
        let result = app.session.active().ok_or("result lost")?;
        assert_eq!(result.wb.sheets[0].cells.get(cell).ok_or("cell")?.value, gridcraft_engine::core::Value::Text("板材".into()));
        assert!(result.path.is_none() && result.is_dirty());
        Ok(())
    }
    #[test]
    fn empty_and_cancelled_results_do_not_create_workbooks() -> Result<(), String> {
        let mut app = fixture(Arc::new(|_, _, _| Ok(Translation { skipped_bilingual: 2, ..Default::default() })))?;
        app.run("ui.translateWorkbook", json!({}))?;
        wait(&mut app)?;
        assert_eq!(app.translation.status["state"], "skipped");
        assert_eq!(app.session.documents().len(), 1);
        app.services.translate_workbook = Some(Arc::new(|request, _, flag| {
            while !flag.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            Ok(Translation {
                bytes: gridcraft_engine::io::save_bytes(&request.wb, "a.xlsx").map_err(|e| e.to_string())?,
                changed: 1,
                ..Default::default()
            })
        }));
        app.run("ui.translateWorkbook", json!({}))?;
        app.run("file.translateEnglish.cancel", json!({}))?;
        wait(&mut app)?;
        assert_eq!(app.translation.status["state"], "cancelled");
        assert_eq!(app.session.documents().len(), 1);
        Ok(())
    }
    #[test]
    fn chinese_button_captures_selected_range_and_direction() -> Result<(), String> {
        let mut app = fixture(Arc::new(|request, _, _| {
            assert_eq!(request.target, TargetLanguage::Chinese);
            assert_eq!(request.scope, Scope::Selection { sheet: 0, ranges: vec![RangeRef::parse("C3:D5").ok_or("range")?] });
            Ok(Translation::default())
        }))?;
        app.run("selection.set", json!({"range":"C3:D5"}))?;
        assert!(app.run("file.translateChinese", json!({})).is_err());
        app.run("ui.translateChinese", json!({}))?;
        wait(&mut app)?;
        assert_eq!(app.translation.status["targetLanguage"], "zh");
        assert_eq!(app.translation.status["state"], "skipped");
        assert_eq!(output_name("a.translated.en.xlsx", TargetLanguage::Chinese), "a.translated.zh.xlsx");
        assert_eq!(output_name("a.translated.zh.xlsx", TargetLanguage::Chinese), "a.translated.zh.xlsx");
        assert_eq!(TargetLanguage::Chinese.api(), ("auto-to-zh", "Chinese"));
        Ok(())
    }
    #[test]
    fn file_open_never_uploads_and_no_automatic_mode_survives_upgrade() -> Result<(), String> {
        let mut app = fixture(Arc::new(|_, _, _| Err("should not run".into())))?;
        assert!(app.run("file.translateEnglish", json!({})).is_err());
        assert!(app.run("options.pcbtool", json!({"mode":"auto","allowHttpUpload":true})).is_err());
        app.ui = serde_json::from_value(json!({"pcbtoolMode":"auto","language":"zh"})).map_err(|e| e.to_string())?;
        let bytes = gridcraft_engine::io::save_bytes(&app.session.active().ok_or("doc")?.wb, "a.xlsx").map_err(|e| e.to_string())?;
        app.run("file.open", json!({"name":"a.xlsx","base64":gridcraft_engine::io::base64_encode(&bytes)}))?;
        assert!(app.translation.job.is_none());
        assert_eq!(output_name("a.translated.en.xlsx", TargetLanguage::English), "a.translated.en.xlsx");
        Ok(())
    }
    #[test]
    fn failure_and_shutdown_preserve_original() -> Result<(), String> {
        let mut app = fixture(Arc::new(|_, _, _| Err("offline".into())))?;
        let uid = app.session.active().ok_or("doc")?.uid;
        app.run("ui.translateWorkbook", json!({}))?;
        wait(&mut app)?;
        assert_eq!(app.translation.status["state"], "failed");
        assert_eq!(app.session.active().ok_or("doc")?.uid, uid);
        let finished = Arc::new(AtomicBool::new(false));
        let observed = finished.clone();
        app.services.translate_workbook = Some(Arc::new(move |_, _, flag| {
            while !flag.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            observed.store(true, Ordering::Relaxed);
            Err("cancelled".into())
        }));
        app.run("ui.translateWorkbook", json!({}))?;
        app.shutdown_translation();
        assert!(finished.load(Ordering::Relaxed));
        Ok(())
    }
}

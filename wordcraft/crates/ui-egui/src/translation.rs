//! Manual translation commands, frozen snapshots, progress and independent result documents.
use crate::WordApp;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use wordcraft_engine::translation::{Language, Request, Scope};

pub type Publisher = Box<dyn Fn(&wordcraft_doc::Document, &str) -> Result<String, String>>;
pub type Progress = Arc<Mutex<String>>;
#[derive(Default)]
pub struct Translation {
    pub doc: Option<wordcraft_doc::Document>,
    pub changed: usize,
    pub skipped_bilingual: usize,
    pub skipped_special: usize,
    pub details: Vec<Value>,
}
pub type Translator = Arc<dyn Fn(Request, Progress, &AtomicBool) -> Result<Translation, String> + Send + Sync>;
struct Job {
    cancel: Arc<AtomicBool>,
    progress: Progress,
    worker: Option<JoinHandle<Result<Translation, String>>>,
    name: String,
    source: u64,
    target: Language,
    scope: &'static str,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub(crate) struct State {
    job: Option<Job>,
    visible: bool,
    status: Value,
    feedback: String,
}
impl Default for State {
    fn default() -> Self {
        Self { job: None, visible: false, status: json!({"state":"idle"}), feedback: String::new() }
    }
}

pub fn commands(app: &WordApp) -> Vec<Value> {
    [
        ("ui.translateSelection","翻译选中区域","{}; current selection to English; explicit send to PCBtool"),
        ("ui.translateDocument","翻译整个文档","{}; document to English; explicit send to PCBtool"),
        ("ui.translateChinese","翻译成中文","{}; current selection to Chinese; explicit send to PCBtool"),
        ("file.translateEnglish","Translate to English","{allowHttpUpload:true,scope?:selection|document}; default document"),
        ("file.translateChinese","Translate to Chinese","{allowHttpUpload:true,scope?:selection|document}; default selection"),
        ("file.translation.status","Translation progress","{} → {state,changedSegments?,skippedBilingual?,skippedSpecial?,targetLanguage?}"),
        ("file.translation.cancel","Cancel translation","{}; stops local waiting, cannot delete server-side data"),
    ].into_iter().map(|(id,label,params)|json!({"id":id,"label":label,"params":params,"location":"Home › 翻译","enabled":app.services.translate_document.is_some(),"mutates":false})).collect()
}
fn output_name(name: &str, target: Language) -> String {
    let stem = std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("Document");
    format!("{}.translated.{}.docx", stem.trim_end_matches(".translated.en").trim_end_matches(".translated.zh"), target.code())
}
impl WordApp {
    pub(crate) fn translation_command(&mut self, id: &str, p: &Value) -> Option<Result<Value, String>> {
        match id {
            "file.translation.status" => {
                let mut status = self.translation.status.clone();
                if let Some(job) = &self.translation.job {
                    status["message"] = json!(job.progress.lock().map(|s| s.clone()).unwrap_or_default());
                }
                Some(Ok(status))
            }
            "file.translation.cancel" => {
                if let Some(job) = &self.translation.job {
                    job.cancel.store(true, Ordering::Relaxed);
                }
                Some(Ok(json!({"cancelRequested":true})))
            }
            "ui.translateSelection" | "ui.translateDocument" | "ui.translateChinese" | "file.translateEnglish" | "file.translateChinese" => {
                self.translation.visible = true;
                if self.translation.job.is_some() {
                    return Some(Ok(self.translation.status.clone()));
                }
                if id.starts_with("file.") && p.get("allowHttpUpload").and_then(Value::as_bool) != Some(true) {
                    return Some(Err("需要 allowHttpUpload:true,确认将待译文字发送到 http://tech0.top:37002/.".into()));
                }
                let target = if id.ends_with("Chinese") { Language::Chinese } else { Language::English };
                let scope = if id.starts_with("ui.") {
                    if id == "ui.translateDocument" { "document" } else { "selection" }
                } else {
                    p.get("scope").and_then(Value::as_str).unwrap_or(if target == Language::Chinese { "selection" } else { "document" })
                };
                let result = self.start_translation(scope, target);
                if let Err(e) = &result {
                    self.translation.feedback = e.clone();
                    self.translation.status = json!({"state":"failed","error":e});
                }
                Some(result)
            }
            _ => None,
        }
    }
    fn start_translation(&mut self, scope: &str, target: Language) -> Result<Value, String> {
        if !self.has_document() {
            return Err("请先打开文档.".into());
        }
        if !self.canvas.ime_preedit.is_empty() {
            return Err("请先完成输入法中的文字,再执行翻译.".into());
        }
        crate::panes::flush_comment_drafts(self)?;
        let service = self.services.translate_document.clone().ok_or("本平台未配置翻译服务.")?;
        if self.services.publish_translation.is_none() {
            return Err("本平台未配置译文副本窗口.".into());
        }
        let (scope, label) = match scope {
            "document" => (Scope::Document, "整个文档"),
            "selection" => {
                let ranges = if let Some(segments) = self.session.column_segments() {
                    segments.iter().map(|(a, b)| wordcraft_engine::Selection { anchor: a.clone(), focus: b.clone() }).collect()
                } else {
                    vec![self.session.sel.clone()]
                };
                (Scope::Selection(ranges), "当前选区")
            }
            _ => return Err("scope 必须为 selection 或 document.".into()),
        };
        let name = self
            .session
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| self.session.file_name.clone())
            .unwrap_or_else(|| self.title_stem());
        let source = self.session.document_id();
        let request = Request { doc: self.session.doc.clone(), scope, target };
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let progress = Arc::new(Mutex::new("正在检查待译文字与双语内容…".into()));
        let worker_progress = progress.clone();
        let worker = std::thread::Builder::new()
            .name("wordcraft-translation".into())
            .spawn(move || service(request, worker_progress, &worker_cancel))
            .map_err(|e| e.to_string())?;
        self.translation.status = json!({"state":"running","pending":true,"sourceId":source,"scope":label,"targetLanguage":target.code()});
        self.translation.feedback.clear();
        self.translation.job = Some(Job { cancel, progress, worker: Some(worker), name, source, target, scope: label });
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
            .ok_or_else(|| "翻译任务丢失.".to_string())
            .and_then(|w| w.join().map_err(|_| "翻译线程异常,原文未改动.".into()))
            .and_then(|r| r);
        let result = if cancelled { Err("已取消本机等待;已发送的服务端任务可能仍在运行.".into()) } else { result };
        match result {
            Ok(mut result) => {
                if result.changed > 0 && result.doc.is_none() {
                    self.translation.status = json!({"state":"failed","error":"没有译文文档"});
                    self.translation.feedback = "没有译文文档,原文不变.".into();
                } else {
                    let name = output_name(&job.name, job.target);
                    let mut output_path = None;
                    if result.changed > 0
                        && let Some(doc) = result.doc.take()
                    {
                        match self
                            .services
                            .publish_translation
                            .as_ref()
                            .ok_or_else(|| "译文窗口不可用".to_string())
                            .and_then(|publish| publish(&doc, &name))
                        {
                            Ok(path) => output_path = Some(path),
                            Err(e) => {
                                self.translation.feedback = e.clone();
                                self.translation.status = json!({"state":"failed","pending":false,"error":e});
                                self.translation.visible = true;
                                self.status(self.translation.feedback.clone());
                                ctx.request_repaint();
                                return;
                            }
                        }
                    }
                    self.translation.feedback = if result.changed == 0 {
                        format!("处理完成:无需补译或已有译文 {} 处;域/控件/修订等未发送 {} 处.", result.skipped_bilingual, result.skipped_special)
                    } else {
                        format!(
                            "已补译 {} 处,已有译文或无需补译 {} 处,特殊段落未发送 {} 处.译文副本已保存并在新窗口打开.",
                            result.changed, result.skipped_bilingual, result.skipped_special
                        )
                    };
                    self.translation.status = json!({"state":if result.changed==0 {"skipped"} else {"succeeded"},"pending":false,"changedSegments":result.changed,"skippedBilingual":result.skipped_bilingual,"skippedSpecial":result.skipped_special,"targetLanguage":job.target.code(),"sourceId":job.source,"name":if result.changed==0 {None} else {Some(name)},"path":output_path,"details":result.details});
                }
            }
            Err(e) => {
                self.translation.feedback = e.clone();
                self.translation.status = json!({"state":if cancelled {"cancelled"} else {"failed"},"pending":false,"error":e});
            }
        }
        self.translation.visible = true;
        self.status(self.translation.feedback.clone());
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
    pub(crate) fn translation_window(&mut self, ctx: &egui::Context) {
        if !self.translation.visible {
            return;
        }
        let mut open = true;
        egui::Window::new("PCBtool · 中英翻译").open(&mut open).default_width(500.0).resizable(false).show(ctx, |ui| {
            ui.hyperlink_to("tech0.top · PCBtool", "http://tech0.top:37002/PCBtool");
            if let Some(job) = &self.translation.job {
                ui.label(format!("{} → {}", job.scope, job.target.label()));
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(job.progress.lock().map(|s| s.clone()).unwrap_or_default());
                });
                ui.label("只发送当前范围的候选文字;AI 检查双语完整性后补译.");
                if ui.button("取消翻译").clicked() {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            } else {
                ui.label(&self.translation.feedback);
                if let Some(path) = self.translation.status.get("path").and_then(Value::as_str) {
                    ui.label(format!("副本:{path}"));
                }
                if let Some(details) = self.translation.status.get("details").and_then(Value::as_array) {
                    ui.collapsing("查看段落处理明细", |ui| {
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for item in details {
                                ui.label(format!(
                                    "文本 {} · {}处 · {}:{}",
                                    item["textIndex"],
                                    item["occurrences"],
                                    item["action"].as_str().unwrap_or(""),
                                    item["reason"].as_str().unwrap_or("")
                                ));
                            }
                        });
                    });
                }
                if ui.button("关闭").clicked() {
                    self.translation.visible = false;
                }
            }
        });
        if !open {
            self.translation.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use wordcraft_doc::{CharProps, Document, Pos};
    fn fixture(service: Translator) -> WordApp {
        let mut session = wordcraft_engine::Session::new(Document::new());
        session.run("text.insert", &json!({"text":"板材","raw":true})).unwrap();
        WordApp::new(
            session,
            Services { translate_document: Some(service), publish_translation: Some(Box::new(|_, _| Ok("test.docx".into()))), ..Default::default() },
        )
    }
    fn wait(app: &mut WordApp) {
        let ctx = egui::Context::default();
        for _ in 0..1000 {
            app.poll_translation(&ctx);
            if app.translation.job.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        app.shutdown_translation();
        panic!("worker timeout");
    }
    #[test]
    fn snapshot_result_is_a_new_dirty_document_and_preserves_later_edits() {
        let ready = Arc::new(AtomicBool::new(false));
        let worker_ready = ready.clone();
        let mut app = fixture(Arc::new(move |request, _, flag| {
            while !worker_ready.load(Ordering::Relaxed) && !flag.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            let mut doc = request.doc;
            doc.insert_text(&Pos::body(0, 6), "\nBoard material", &CharProps::default()).map_err(|e| e.to_string())?;
            Ok(Translation { doc: Some(doc), changed: 1, ..Default::default() })
        }));
        let copies = Arc::new(Mutex::new(Vec::new()));
        let copies_out = copies.clone();
        app.services.publish_translation = Some(Box::new(move |doc, _| {
            copies_out.lock().map_err(|e| e.to_string())?.push(doc.clone());
            Ok("test.docx".into())
        }));
        let source = app.session.document_id();
        app.execute("ui.translateDocument", json!({})).unwrap();
        app.execute("text.insert", json!({"text":"后续修改","raw":true})).unwrap();
        ready.store(true, Ordering::Relaxed);
        wait(&mut app);
        assert_eq!(app.translation.status["state"], "succeeded");
        assert_eq!(source, app.session.document_id());
        assert!(app.session.dirty && app.session.path.is_none());
        assert_eq!(copies.lock().unwrap()[0].para_at(&Pos::body(0, 0)).unwrap().text, "板材\nBoard material");
        assert_eq!(app.session.doc.para_at(&Pos::body(0, 0)).unwrap().text, "板材后续修改");
    }
    #[test]
    fn chinese_button_captures_only_selected_text_and_programmatic_send_is_explicit() {
        let mut app = fixture(Arc::new(|r, _, _| {
            assert_eq!(r.target, Language::Chinese);
            let Scope::Selection(s) = r.scope else { panic!("scope") };
            assert_eq!(s[0].anchor, Pos::body(0, 0));
            assert_eq!(s[0].focus, Pos::body(0, 3));
            Ok(Translation::default())
        }));
        app.session.sel = wordcraft_engine::Selection { anchor: Pos::body(0, 0), focus: Pos::body(0, 3) };
        assert!(app.execute("file.translateChinese", json!({})).is_err());
        app.execute("ui.translateChinese", json!({})).unwrap();
        wait(&mut app);
        assert_eq!(app.translation.status["state"], "skipped");
        assert_eq!(app.session.doc.para_at(&Pos::body(0, 0)).unwrap().text, "板材");
        assert_eq!(output_name("Test.translated.en.docx", Language::Chinese), "Test.translated.zh.docx");
    }
    #[test]
    fn failures_and_cancel_never_open_partial_results() {
        let mut app = fixture(Arc::new(|_, _, _| Err("offline".into())));
        app.execute("ui.translateDocument", json!({})).unwrap();
        wait(&mut app);
        assert_eq!(app.translation.status["state"], "failed");
        assert_eq!(app.session.doc.para_at(&Pos::body(0, 0)).unwrap().text, "板材");
        app.services.translate_document = Some(Arc::new(|_, _, flag| {
            while !flag.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            Err("cancelled".into())
        }));
        app.execute("ui.translateDocument", json!({})).unwrap();
        app.execute("file.translation.cancel", json!({})).unwrap();
        wait(&mut app);
        assert_eq!(app.translation.status["state"], "cancelled");
        assert_eq!(app.session.doc.para_at(&Pos::body(0, 0)).unwrap().text, "板材");
        app.execute("ui.translateDocument", json!({})).unwrap();
        app.shutdown_translation();
        assert!(app.translation.job.is_none());
    }

    #[test]
    fn ribbon_click_keeps_the_exact_unicode_selection_in_both_directions() {
        use egui_kittest::kittest::Queryable;
        for (source, selected, button, target) in [
            ("不应发送 中文\u{1f600}PTH孔径 后面的内容", "中文\u{1f600}PTH孔径", "翻译选中区域", Language::English),
            ("不应发送 Board thickness 1.6mm 后面的内容", "Board thickness 1.6mm", "翻译成中文", Language::Chinese),
        ] {
            for reversed in [false, true] {
                let captured = Arc::new(Mutex::new(Vec::new()));
                let captured_worker = captured.clone();
                let mut doc = Document::new();
                doc.body = vec![wordcraft_doc::para_block(wordcraft_doc::Paragraph::with_text(source, CharProps::default()))];
                let mut session = wordcraft_engine::Session::new(doc);
                let start = source.find(selected).unwrap();
                let a = Pos::body(0, start);
                let b = Pos::body(0, start + selected.len());
                session.sel =
                    if reversed { wordcraft_engine::Selection { anchor: b, focus: a } } else { wordcraft_engine::Selection { anchor: a, focus: b } };
                let selection = session.sel.clone();
                let mut app = WordApp::new(
                    session,
                    Services {
                        translate_document: Some(Arc::new(move |r, _, _| {
                            let plan = wordcraft_engine::translation::plan(&r)?;
                            captured_worker.lock().map_err(|e| e.to_string())?.push((r.target, plan.texts));
                            Ok(Translation::default())
                        })),
                        publish_translation: Some(Box::new(|_, _| Err("no output expected".into()))),
                        ..Default::default()
                    },
                );
                app.ui.language = "zh-hans".into();
                let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1920.0, 900.0)).build_ui_state(
                    |ui, app: &mut WordApp| {
                        let ctx = ui.ctx().clone();
                        app.logic(&ctx);
                        app.ui(ui);
                    },
                    app,
                );
                h.run_steps(4);
                h.get_by_label(button).click();
                h.run_steps(4);
                wait(h.state_mut());
                assert_eq!(*captured.lock().unwrap(), vec![(target, vec![selected.to_string()])]);
                assert_eq!(h.state().session.sel, selection, "ribbon click must not move the selected range");
                assert_eq!(h.state().session.doc.para_at(&Pos::body(0, 0)).unwrap().text, source);
            }
        }
    }
}

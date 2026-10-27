//! Asynchronous desktop import service. The spreadsheet engine remains platform-independent.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;

use serde_json::{Value as Json, json};

use crate::{Notice, NoticeKind, SheetApp};

pub type Converter = Arc<dyn Fn(&str, &AtomicBool) -> Result<Vec<u8>, String> + Send + Sync>;

pub(crate) struct Job {
    source: String,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Vec<u8>, String>>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub fn is_xls(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("xls"))
}

impl SheetApp {
    pub(crate) fn start_xls_import(&mut self, path: &str) -> Result<Json, String> {
        if !is_xls(path) {
            return Err("兼容入口仅接受 .xls 文件。".into());
        }
        if self.xls_import.is_some() {
            return Err("正在兼容导入另一个 XLS，请完成后重试。".into());
        }
        let convert = self.services.convert_xls.clone().ok_or("本平台未配置 LibreOffice 兼容导入服务。")?;
        let source = path.to_string();
        let worker_source = source.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = std::thread::Builder::new()
            .name("xls-compat".into())
            .spawn(move || convert(&worker_source, &worker_cancelled))
            .map_err(|e| format!("无法启动兼容导入：{e}"))?;
        self.xls_import = Some(Job { source: source.clone(), cancelled, worker: Some(worker) });
        self.xls_import_status = json!({"state":"running", "pending":true, "source":source});
        self.backstage = None;
        self.notice =
            Some(Notice {
                text: "正在通过本机 LibreOffice 导入 XLS… 原文件不会被修改，请稍候。".into(), kind: NoticeKind::Info, action: None
            });
        Ok(self.xls_import_status.clone())
    }

    pub(crate) fn poll_xls_import(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.xls_import else { return };
        if !job.worker.as_ref().is_some_and(|worker| worker.is_finished()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        let Some(mut job) = self.xls_import.take() else { return };
        let result = job
            .worker
            .take()
            .ok_or_else(|| "兼容导入任务丢失。".to_string())
            .and_then(|worker| worker.join().map_err(|_| "兼容转换线程异常；原文件未更改。".to_string()))
            .and_then(|r| r)
            .and_then(|bytes| {
                let stem = std::path::Path::new(&job.source).file_stem().and_then(|s| s.to_str()).unwrap_or("Book");
                let name = format!("{stem} (LibreOffice 导入).xlsx");
                // Import from bytes: no temporary path becomes a save target or Recent entry.
                self.run("file.open", json!({"name":name, "base64":gridcraft_engine::io::base64_encode(&bytes)}))
            });
        match result {
            Ok(r) => {
                self.after_open(Some(&job.source), &r);
                let warnings = r.get("warnings").cloned().unwrap_or_else(|| json!([]));
                self.xls_import_status = json!({"state":"succeeded", "pending":false, "source":job.source, "warnings":warnings});
                let prior = self.notice.as_ref().map(|n| format!("\n{}", n.text)).unwrap_or_default();
                self.notice = Some(Notice {
                    text: format!(
                        "已通过 LibreOffice 导入；请另存为 XLSX。原 XLS 未更改。宏不执行、不保留；复杂对象及格式仍可能存在差异，请核对。{prior}"
                    ),
                    kind: NoticeKind::Warning,
                    action: None,
                });
            }
            Err(e) => {
                self.xls_import_status = json!({"state":"failed", "pending":false, "source":job.source, "error":e});
                self.notice =
                    Some(Notice {
                        text: "XLS 兼容导入失败，原文件未更改；未自动降级到简化导入。".into(), kind: NoticeKind::Warning, action: None
                    });
                self.message = Some(("XLS 兼容导入".into(), e));
            }
        }
        self.after_engine();
        ctx.request_repaint();
    }

    /// Called before the desktop shell exits, including its macOS process::exit path.
    /// Cancellation kills only this job's isolated LibreOffice process and cleans its files.
    pub fn shutdown_xls_import(&mut self) {
        if let Some(mut job) = self.xls_import.take() {
            job.cancelled.store(true, Ordering::Relaxed);
            if let Some(worker) = job.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;

    fn await_import(app: &mut SheetApp) -> Result<(), String> {
        let ctx = egui::Context::default();
        #[allow(clippy::disallowed_methods)] // Native worker regression test, not a WASM entry.
        let start = std::time::Instant::now();
        while app.xls_import.is_some() && start.elapsed().as_secs() < 10 {
            app.poll_xls_import(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        if app.xls_import.is_some() {
            return Err("import timed out".into());
        }
        Ok(())
    }

    #[test]
    fn converted_workbook_has_no_save_target_and_recent_points_to_original() -> Result<(), String> {
        let mut session = gridcraft_engine::Session::new();
        session.new_workbook();
        session.run("cell.set", json!({"cell":"A1","input":"42"})).map_err(|e| e.to_string())?;
        let r = session.run("file.saveBytes", json!({"format":"xlsx"})).map_err(|e| e.to_string())?;
        let bytes = r.get("base64").and_then(Json::as_str).and_then(gridcraft_engine::io::base64_decode).ok_or("no bytes")?;
        let mut app = SheetApp::new(
            gridcraft_engine::Session::new(),
            Services { convert_xls: Some(Arc::new(move |_, _| Ok(bytes.clone()))), ..Default::default() },
        );
        let source = "/test/中文 ' quoted.XLS";
        app.open_path(source);
        assert!(app.xls_import.is_some());
        assert!(app.start_xls_import(source).is_err());
        await_import(&mut app)?;
        assert_eq!(app.xls_import_status["state"], "succeeded");
        let doc = app.session.active().ok_or("no document")?;
        assert!(doc.path.is_none());
        assert!(doc.display_title().ends_with("(LibreOffice 导入).xlsx"));
        assert_eq!(app.ui.recent.first().map(String::as_str), Some(source));
        assert_eq!(app.session.documents().len(), 1);
        Ok(())
    }

    #[test]
    fn failure_keeps_open_document_and_reports_error() -> Result<(), String> {
        let mut session = gridcraft_engine::Session::new();
        session.new_workbook();
        let uid = session.active().ok_or("no document")?.uid;
        let mut app =
            SheetApp::new(session, Services { convert_xls: Some(Arc::new(|_, _| Err("converter unavailable".into()))), ..Default::default() });
        app.open_path("bad.xls");
        await_import(&mut app)?;
        assert_eq!(app.xls_import_status["state"], "failed");
        assert_eq!(app.session.active().ok_or("no document")?.uid, uid);
        assert!(app.message.as_ref().is_some_and(|(_, e)| e == "converter unavailable"));
        Ok(())
    }

    #[test]
    fn command_needs_path_without_opening_dialogs() {
        let mut app = SheetApp::new(gridcraft_engine::Session::new(), Services::default());
        assert!(app.run("file.importXls", json!({})).is_err());
        assert!(app.dialog.is_none());
        assert!(is_xls("file.XlS"));
        assert!(!is_xls("file.xlsx"));
    }

    #[test]
    fn other_extensions_do_not_call_converter() {
        let called = Arc::new(AtomicBool::new(false));
        let observed = called.clone();
        let mut app = SheetApp::new(
            gridcraft_engine::Session::new(),
            Services {
                convert_xls: Some(Arc::new(move |_, _| {
                    observed.store(true, Ordering::Relaxed);
                    Err("should not run".into())
                })),
                ..Default::default()
            },
        );
        app.open_path("/nonexistent/file.xlsx");
        assert!(!called.load(Ordering::Relaxed));
        assert!(app.xls_import.is_none());
    }

    #[test]
    fn shutdown_cancels_and_joins_converter() -> Result<(), String> {
        let finished = Arc::new(AtomicBool::new(false));
        let observed = finished.clone();
        let mut app = SheetApp::new(
            gridcraft_engine::Session::new(),
            Services {
                convert_xls: Some(Arc::new(move |_, cancelled| {
                    while !cancelled.load(Ordering::Relaxed) {
                        std::thread::yield_now();
                    }
                    observed.store(true, Ordering::Relaxed);
                    Err("cancelled".into())
                })),
                ..Default::default()
            },
        );
        app.start_xls_import("test.xls")?;
        app.shutdown_xls_import();
        assert!(finished.load(Ordering::Relaxed));
        assert!(app.xls_import.is_none());
        Ok(())
    }
}

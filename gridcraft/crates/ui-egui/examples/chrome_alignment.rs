//! Offline visual QA for the API chrome; the mock services cannot send any data.
//! cargo run -p gridcraft-ui-egui --example chrome_alignment -- out.png [--settings] [--dark]

use std::sync::Arc;

use gridcraft_engine::Session;
use gridcraft_ui_egui::{Services, SheetApp, i18n::Language};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = args.first().ok_or("provide output PNG path")?;
    let mut session = Session::new();
    session.new_workbook();
    for (cell, input) in [("A1", "中文与 English"), ("A2", "输入位置 abc123"), ("B1", "API 对齐检查"), ("B2", "保留单元格原格式")]
    {
        session.execute("cell.set", json!({"cell": cell, "input": input}))?;
    }
    session.execute("home.columnWidth", json!({"range": "A:B", "chars": 28}))?;
    session.execute("selection.set", json!({"range": "A1:B2"}))?;
    let services = Services {
        ai_complete: Some(Arc::new(|_, _, _| Err("offline visual QA only".into()))),
        translate_workbook: Some(Arc::new(|_, _, _| Err("offline visual QA only".into()))),
        ..Default::default()
    };
    let mut app = SheetApp::new(session, services);
    app.ui.language = Language::Zh;
    app.ui.dark = args.iter().any(|a| a == "--dark");
    app.ai.visible = !args.iter().any(|a| a == "--settings");
    app.ai.settings_open = args.iter().any(|a| a == "--settings");
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_pixels_per_point(2.0).wgpu().build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(14);
    h.render()?.save(output)?;
    Ok(())
}

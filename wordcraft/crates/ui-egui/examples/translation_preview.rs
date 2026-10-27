use std::sync::Arc;
use wordcraft_ui_egui::{Services, WordApp};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = wordcraft_engine::Session::new(wordcraft_doc::Document::new());
    session.run("text.insert", &serde_json::json!({"text":"阻焊颜色\nSolder mask color","raw":true}))?;
    let mut app = WordApp::new(session, Services { translate_document: Some(Arc::new(|_, _, _| Err("Preview only".into()))), ..Default::default() });
    app.ui.language = "zh-hans".into();
    let mut h =
        egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_pixels_per_point(1.5).with_max_steps(12).wgpu().build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
    h.run_steps(8);
    h.render()?.save(std::env::args().nth(1).ok_or("output path")?)?;
    Ok(())
}

use gridcraft_ui_egui::{Services, SheetApp};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = gridcraft_engine::Session::new();
    session.new_workbook();
    session.run("cell.set", json!({"cell":"A1","input":"PTH孔径公差±0.075mm，标记YWW\nPTH hole diameter tolerance ±0.075mm, mark YWW"}))?;
    session.run("home.columnWidth", json!({"cols":"A:A","width":600}))?;
    session.run("home.fontSize", json!({"range":"A1","size":14}))?;
    session.run("home.wrapText", json!({"range":"A1","on":true}))?;
    session.run("home.alignCenter", json!({"range":"A1"}))?;
    let mut app = SheetApp::new(
        session,
        Services { translate_workbook: Some(std::sync::Arc::new(|_, _, _| Err("Preview only".into()))), ..Default::default() },
    );
    app.ui.language = gridcraft_ui_egui::i18n::Language::Zh;
    let mut h =
        egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_pixels_per_point(1.0).with_max_steps(12).wgpu().build_ui_state(
            |ui, app: &mut SheetApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
    h.run_steps(14);
    h.render()?.save(std::env::args().nth(1).ok_or("output path")?)?;
    Ok(())
}

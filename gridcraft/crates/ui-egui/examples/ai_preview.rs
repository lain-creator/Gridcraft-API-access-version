//! Synthetic assistant preview; no API key, user workbook or external request.
use egui_kittest::kittest::Queryable as _;
use gridcraft_ui_egui::{Services, SheetApp};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("output PNG")?;
    let settings = std::env::args().any(|s| s == "--settings");
    let minimized = std::env::args().any(|s| s == "--minimized");
    let charts = std::env::args().any(|s| s == "--charts");
    let analysis = std::env::args().any(|s| s == "--analysis");
    let _ = gridcraft_ui_egui::fonts::catalog();
    let mut session = gridcraft_engine::Session::new();
    session.new_workbook();
    for (cell, input) in [
        ("A1", "产品"),
        ("B1", "数量"),
        ("C1", "单价"),
        ("D1", "金额"),
        ("A2", "Pogo PCB"),
        ("B2", "1000"),
        ("C2", "4.78"),
        ("D2", "=B2*C2"),
        ("A3", "工装费"),
        ("B3", "1"),
        ("C3", "500"),
        ("D3", "=B3*C3"),
    ] {
        session.run("cell.set", json!({"cell":cell,"input":input}))?;
    }
    session.run("selection.set", json!({"range":"A1:D3"}))?;
    let mut app = SheetApp::new(
        session,
        Services {
            ai_complete: Some(std::sync::Arc::new(move |_, _, _| {
                if charts {
                    return Ok(json!({"charts":[{"title":"产品金额对比","chartType":"bar","xColumn":"A","yColumns":["D"],"aggregation":"none","sort":"valueDesc","reason":"比较两个项目的金额，横向条形图方便阅读产品名称；按金额降序排列。","xTitle":"项目","yTitle":"金额"},{"title":"产品金额占比","chartType":"doughnut","xColumn":"A","yColumns":["D"],"aggregation":"none","sort":"source","reason":"两项均为正数，可用环形图查看各项目贡献。","xTitle":"项目","yTitle":"金额"}]}).to_string());
                }
                Ok("框选区域的计算如下：\nD2 = 1000 × 4.78 = 4,780\nD3 = 1 × 500 = 500\n两项金额合计为 5,280。\n以上为合成数据的界面演示回答。"
                    .into())
            })),
            translate_workbook: Some(std::sync::Arc::new(|_, _, _| Err("preview only".into()))),
            ..Default::default()
        },
    );
    app.ui.language = gridcraft_ui_egui::i18n::Language::Zh;
    if charts {
        app.run("ui.ai.charts.generate", json!({"goal":"比较各产品金额，并推荐适合的图表"}))?;
    } else if analysis {
        app.run("ui.ai.analyze.run", json!({"goal":"核对金额，比较两个项目的贡献"}))?;
    } else {
        app.ask_ai("请核对框选内容中的金额，并计算合计。")?;
    }
    app.ai.settings_open = settings;
    app.ai.minimized = minimized;
    let mut harness =
        egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_pixels_per_point(1.0).with_max_steps(16).wgpu().build_ui_state(
            |ui, app: &mut SheetApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
    harness.run_steps(16);
    if charts && std::env::args().any(|s| s == "--full-preview") {
        harness.get_by_label("放大预览").click();
        harness.run_steps(3);
    }
    harness.render()?.save(output)?;
    Ok(())
}

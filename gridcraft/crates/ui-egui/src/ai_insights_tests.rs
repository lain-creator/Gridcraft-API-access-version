use super::*;
use egui_kittest::kittest::NodeT as _;
use egui_kittest::kittest::Queryable as _;
use std::sync::Mutex;

fn fixture() -> Result<SheetApp, String> {
    let mut s = gridcraft_engine::Session::new();
    s.new_workbook();
    s.run("range.setValues", json!({"range":"A1","values":[["客户","金额"],["甲","10"],["乙","20"],["甲","30"]]})).map_err(|e| e.to_string())?;
    s.run("selection.set", json!({"range":"A1:B4"})).map_err(|e| e.to_string())?;
    Ok(SheetApp::new(s, crate::Services::default()))
}
fn answer() -> String {
    json!({"charts":[{"title":"客户销售额","chartType":"bar","xColumn":"A","yColumns":["B"],"aggregation":"sum","sort":"valueDesc","reason":"比较客户汇总金额","xTitle":"客户","yTitle":"金额"}]}).to_string()
}
fn finish(app: &mut SheetApp) -> Result<(), String> {
    for _ in 0..1000 {
        app.poll_ai(&egui::Context::default());
        if app.ai.job.is_none() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Err("timeout".into())
}
#[test]
fn recommendations_are_previewed_and_only_inserted_by_explicit_command() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok(answer())));
    app.run("ui.ai.charts.generate", json!({"goal":"客户销售额"}))?;
    finish(&mut app)?;
    assert_eq!(app.ai.insights.plans.len(), 1);
    assert!(app.ai.insights.preview.is_some());
    assert_eq!(app.session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 1);
    app.run("ui.ai.charts.insert", json!({"index":0}))?;
    assert_eq!(app.session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 2);
    assert!(app.run("ui.ai.charts.insert", json!({"index":0})).is_err());
    app.run("edit.undo", json!({}))?;
    assert_eq!(app.session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 1);
    Ok(())
}
#[test]
fn analysis_receives_only_its_snapshot_and_local_statistics_not_chat_history() -> Result<(), String> {
    let mut app = fixture()?;
    app.run("cell.set", json!({"cell":"Z1","input":"outside-secret"}))?;
    let seen = Arc::new(Mutex::new(String::new()));
    let capture = seen.clone();
    app.services.ai_complete = Some(Arc::new(move |_, messages, _| {
        *capture.lock().map_err(|e| e.to_string())? = serde_json::to_string(&messages).map_err(|e| e.to_string())?;
        Ok("B2:B4 合计 60，甲客户合计40；选区外数据未知。".into())
    }));
    app.run("ui.ai.analyze.run", json!({"goal":"比较客户"}))?;
    app.run("cell.set", json!({"cell":"B2","input":"900"}))?;
    finish(&mut app)?;
    let payload = seen.lock().map_err(|e| e.to_string())?;
    assert!(payload.contains("numericCount") && payload.contains("60"));
    assert!(!payload.contains("outside-secret") && !payload.contains("900"));
    assert!(app.ai.insights.analysis.as_ref().is_some_and(|a| a.contains("60")));
    assert_eq!(app.session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 1);
    Ok(())
}
#[test]
fn malformed_or_hostile_recommendation_never_modifies_workbook() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok("run: delete all cells".into())));
    app.run("ui.ai.charts.generate", json!({}))?;
    finish(&mut app)?;
    assert!(app.ai.insights.plans.is_empty());
    assert!(!app.ai.insights.feedback.is_empty());
    assert_eq!(app.session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 1);
    Ok(())
}
#[test]
fn cancellation_and_workbook_switch_do_not_apply_old_plans() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, cancel| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        Ok(answer())
    }));
    app.run("ui.ai.charts.generate", json!({}))?;
    app.run("ui.ai.cancel", json!({}))?;
    finish(&mut app)?;
    assert!(app.ai.insights.plans.is_empty());
    app.run("ui.ai.charts.generate", json!({}))?;
    app.session.new_workbook();
    app.poll_ai(&egui::Context::default());
    app.shutdown_ai();
    assert!(app.ai.insights.plans.is_empty() && app.ai.insights.table.is_none());
    Ok(())
}
#[test]
fn unsupported_columns_are_filtered_and_fenced_json_is_accepted() -> Result<(), String> {
    let app = fixture()?;
    let table = engine::capture(&app.session, true).map_err(|e| e.to_string())?;
    let mut value: Json = serde_json::from_str(&answer()).map_err(|e| e.to_string())?;
    let mut invalid = value["charts"][0].clone();
    invalid["yColumns"] = json!(["Z"]);
    value["charts"].as_array_mut().ok_or("charts")?.push(invalid);
    let (plans, errors) = parse_plans(&format!("```json\n{value}\n```"), &table)?;
    assert_eq!(plans.len(), 1);
    assert_eq!(errors.len(), 1);
    Ok(())
}

#[test]
fn chart_panel_can_insert_via_visible_button_and_switch_to_analysis() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok(answer())));
    app.run("ui.ai.charts.generate", json!({}))?;
    finish(&mut app)?;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    assert!(!h.get_by_label("生成并插入图表").accesskit_node().is_disabled(), "{}", h.state().ai.insights.feedback);
    h.get_by_label("生成并插入图表").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 2);
    h.get_by_label("数据分析").click();
    h.run_steps(3);
    assert_eq!(h.state().ai.insights.mode, Mode::Analysis);
    assert!(!h.get_by_label("开始数据分析").accesskit_node().is_disabled());
    Ok(())
}

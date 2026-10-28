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
    json!({"answer":"B2:B4 合计60，可按客户比较。","hasHeaders":true,"transposed":false,"charts":[{"title":"客户销售额","chartType":"bar","xColumn":"A","yColumns":["B"],"aggregation":"sum","sort":"valueDesc","reason":"比较客户汇总金额","xTitle":"客户","yTitle":"金额"}]}).to_string()
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
fn conversation_receives_immutable_selection_and_local_statistics() -> Result<(), String> {
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
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok("{\"run\":\"delete all cells\"}".into())));
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
    let mut app = fixture()?;
    let mut value: Json = serde_json::from_str(&answer()).map_err(|e| e.to_string())?;
    let mut invalid = value["charts"][0].clone();
    invalid["yColumns"] = json!(["Z"]);
    value["charts"].as_array_mut().ok_or("charts")?.push(invalid);
    app.services.ai_complete = Some(Arc::new(move |_, _, _| Ok(format!("```json\n{value}\n```"))));
    app.ask_ai("分析并画图")?;
    finish(&mut app)?;
    assert_eq!(app.ai.insights.plans.len(), 1);
    assert!(app.ai.insights.feedback.contains('Z'));
    Ok(())
}

#[test]
fn unified_conversation_contains_chart_and_one_composer() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok(answer())));
    app.run("ui.ai.charts.generate", json!({}))?;
    finish(&mut app)?;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 1200.0)).build_ui_state(
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
    assert!(h.query_by_label("开始数据分析").is_none());
    assert!(h.query_by_label("分析并推荐图表").is_none());
    assert!(h.query_by_label("发送问题").is_some());
    Ok(())
}

#[test]
fn horizontal_dates_are_eleven_points_not_one_record() -> Result<(), String> {
    let mut app = fixture()?;
    app.run("cell.set", json!({"cell":"D3","input":"张三"}))?;
    for i in 0..11 {
        app.run("cell.set", json!({"cell":CellRef::new(1,4+i).a1(),"input":(46300+i).to_string()}))?;
        app.run("cell.set", json!({"cell":CellRef::new(2,4+i).a1(),"input":(i+1).to_string()}))?;
    }
    app.run("home.numberFormat", json!({"range":"E2:O2","code":"d-mmm"}))?;
    app.run("selection.set", json!({"range":"D2:O3"}))?;
    app.services.ai_complete = Some(Arc::new(|_, messages, _| {
        let payload = &messages.last().ok_or("payload")?.content;
        assert!(payload.contains("R2") && payload.contains("R3") && payload.contains("layouts"));
        Ok(json!({"answer":"11天合计66，可以看每日变化，不足以预测未来。","hasHeaders":true,"transposed":true,"charts":[{"title":"每日数量","chartType":"line","xColumn":"R2","yColumns":["R3"],"reason":"日期横向展开为11个点"}]}).to_string())
    }));
    app.ask_ai("分析并生成图表")?;
    finish(&mut app)?;
    let table = app.ai.insights.table.as_ref().ok_or("table")?;
    assert!(table.transposed && table.has_headers);
    assert_eq!(table.rows.len(), 11);
    assert_eq!(table.columns.get(1).and_then(|c| c.sum), Some(66.0));
    assert_eq!(table.rows.first().and_then(|r| r.get(1)).map(|c| c.address.as_str()), Some("E3"));
    app.run("ui.ai.charts.insert", json!({}))?;
    let d = app.session.doc().map_err(|e| e.to_string())?;
    let source = d.wb.sheet(0).ok_or("sheet")?;
    let c = source.charts.first().ok_or("chart")?;
    assert_eq!(gridcraft_chart::resolve(&d.wb, 0, c).categories.len(), 11);
    app.run("edit.undo", json!({}))?;
    assert_eq!(app.session.doc().map_err(|e| e.to_string())?.wb.sheets.len(), 1);
    Ok(())
}

#[test]
fn small_and_irregular_selections_still_receive_analysis_without_charts() -> Result<(), String> {
    for range in ["A2", "A2:B2", "A1:B1"] {
        let mut app = fixture()?;
        app.run("selection.set", json!({"range":range}))?;
        app.services.ai_complete =
            Some(Arc::new(|_, _, _| Ok(json!({"answer":"已分析现有单元格；仅有一个样本，不能推断趋势。","charts":[]}).to_string())));
        app.ask_ai("分析这几个格子")?;
        finish(&mut app)?;
        assert!(app.ai.turns.last().and_then(|t| t.error.as_ref()).is_none());
        assert!(app.ai.insights.analysis.is_some() && app.ai.insights.plans.is_empty());
    }
    let mut app = fixture()?;
    app.session.active_mut().ok_or("doc")?.selection.ranges = vec![RangeRef::parse("A2").ok_or("range")?, RangeRef::parse("B3").ok_or("range")?];
    app.services.ai_complete = Some(Arc::new(|_, m, _| {
        assert!(m.last().ok_or("payload")?.content.contains("tableUnavailableReason"));
        Ok(json!({"answer":"甲客户和B3金额可以分别解释；不连续区域不自动画图。","charts":[]}).to_string())
    }));
    app.ask_ai("解释这些内容")?;
    finish(&mut app)?;
    assert!(app.ai.insights.analysis.is_some());
    Ok(())
}

#[test]
fn followup_includes_chart_context_and_empty_charts_are_success() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok(answer())));
    app.ask_ai("生成客户金额图")?;
    finish(&mut app)?;
    app.services.ai_complete = Some(Arc::new(|_, m, _| {
        assert_eq!(m.len(), 4);
        assert!(m.get(2).ok_or("history")?.content.contains("xColumn"));
        Ok(json!({"answer":"甲客户合计40，占本次合计的三分之二。","hasHeaders":true,"charts":[]}).to_string())
    }));
    app.ask_ai("解释一下甲客户")?;
    finish(&mut app)?;
    assert!(app.ai.insights.feedback.is_empty() && app.ai.insights.plans.is_empty());
    assert_eq!(app.ai.turns.len(), 2);
    Ok(())
}

#[test]
fn malformed_chart_does_not_discard_valid_analysis_and_merged_text_is_allowed() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete =
        Some(Arc::new(|_, _, _| Ok(json!({"answer":"合计60，原始数值未修改。","hasHeaders":true,"charts":[{"code":"delete_all()"}]}).to_string())));
    app.ask_ai("分析并画图")?;
    finish(&mut app)?;
    assert_eq!(app.ai.turns.last().and_then(|t| t.answer.as_deref()), Some("合计60，原始数值未修改。"));
    assert!(app.ai.insights.plans.is_empty() && !app.ai.insights.feedback.is_empty());
    Arc::make_mut(&mut app.session.active_mut().ok_or("doc")?.wb).sheet_mut(0).ok_or("sheet")?.merges.push(RangeRef::parse("A1:B1").ok_or("range")?);
    app.services.ai_complete = Some(Arc::new(|_, m, _| {
        assert!(m.last().ok_or("payload")?.content.contains("合并单元格"));
        Ok(json!({"answer":"这是客户与金额的数据，合并表头需先确认口径。","charts":[]}).to_string())
    }));
    app.ask_ai("解释这份表")?;
    finish(&mut app)?;
    assert!(app.ai.insights.analysis.as_ref().is_some_and(|a| a.contains("客户")));
    Ok(())
}

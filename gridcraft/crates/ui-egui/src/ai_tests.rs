use super::*;
use crate::Services;
use egui_kittest::kittest::Queryable as _;
use std::sync::Mutex;

#[test]
fn glossary_command_is_local_and_never_opens_a_picker_programmatically() -> Result<(), String> {
    let mut app = fixture()?;
    let paths = Arc::new(Mutex::new(Vec::new()));
    let seen = paths.clone();
    app.services.ai_glossary = Some(Box::new(move |path| {
        seen.lock().map_err(|e| e.to_string())?.push(path.map(str::to_string));
        Ok(GlossaryInfo { entries: 8, quarantined: 1, ambiguous: 1, source: "synthetic.json".into() })
    }));
    app.services.pick_glossary = Some(Box::new(|| panic!("programmatic calls must not open dialogs")));
    app.services.ai_complete = Some(Arc::new(|_, _, _| panic!("glossary import must not call an API")));
    let count = app.session.documents().len();
    assert_eq!(app.run("ui.ai.glossary", json!({}))?["entries"], 8);
    app.run("ui.ai.glossary", json!({"path":"/synthetic/pcb.json"}))?;
    assert!(app.run("ui.ai.glossary", json!({"path":7})).is_err());
    assert!(app.run("ui.ai.glossary", json!({"path":""})).is_err());
    assert_eq!(*paths.lock().map_err(|e| e.to_string())?, vec![None, Some("/synthetic/pcb.json".into())]);
    assert_eq!(app.session.documents().len(), count);
    Ok(())
}

#[test]
fn glossary_settings_layout_has_visible_import_and_refresh() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_glossary = Some(Box::new(|_| Ok(GlossaryInfo { entries: 733, quarantined: 1, ambiguous: 40, source: "PCBtool.json".into() })));
    app.run("ui.ai.settings", json!({}))?;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h.get_by_label("刷新词库状态").click();
    h.run_steps(3);
    assert!(h.state().ai.glossary_status.contains("733"));
    assert!(h.query_by_label("导入词库 JSON…").is_some());
    Ok(())
}

fn fixture() -> Result<SheetApp, String> {
    let mut session = Session::new();
    session.new_workbook();
    for (cell, input) in [("A1", "板厚1.6mm"), ("A2", "3"), ("A3", "=A2*2"), ("B1", "不在选区"), ("C5", "第二选区")] {
        session.run("cell.set", json!({"cell":cell,"input":input})).map_err(|e| e.to_string())?;
    }
    session.run("selection.set", json!({"range":"A1:A3"})).map_err(|e| e.to_string())?;
    Ok(SheetApp::new(session, Services::default()))
}
#[test]
fn selection_has_values_and_formulas_but_no_outside_cells() -> Result<(), String> {
    let app = fixture()?;
    let s = capture_selection(&app.session)?;
    assert_eq!(s.cells.iter().map(|c| c.address.as_str()).collect::<Vec<_>>(), ["A1", "A2", "A3"]);
    assert_eq!(s.cells.get(1).ok_or("A2")?.text, "3");
    assert_eq!(s.cells.get(2).ok_or("A3")?.formula.as_deref(), Some("=A2*2"));
    assert!(!serde_json::to_string(&s).map_err(|e| e.to_string())?.contains("不在选区"));
    Ok(())
}
#[test]
fn disjoint_and_merged_ranges_are_resolved_and_deduplicated() -> Result<(), String> {
    let mut app = fixture()?;
    {
        let d = app.session.active_mut().ok_or("doc")?;
        Arc::make_mut(&mut d.wb).sheet_mut(0).ok_or("sheet")?.merges.push(RangeRef::parse("A1:C1").ok_or("range")?);
        d.selection.ranges =
            vec![RangeRef::parse("C1").ok_or("range")?, RangeRef::parse("A1").ok_or("range")?, RangeRef::parse("C5").ok_or("range")?];
    }
    let s = capture_selection(&app.session)?;
    assert_eq!(s.cells.iter().map(|c| c.address.as_str()).collect::<Vec<_>>(), ["A1", "C5"]);
    Ok(())
}
#[test]
fn huge_selections_fail_without_silently_truncating() -> Result<(), String> {
    let mut app = fixture()?;
    let d = app.session.active_mut().ok_or("doc")?;
    let sheet = Arc::make_mut(&mut d.wb).sheet_mut(0).ok_or("sheet")?;
    for row in 0..1001 {
        sheet.set_value(CellRef::new(row, 0), Value::Number(1.0));
    }
    d.selection.ranges = vec![RangeRef::parse("A1:A1001").ok_or("range")?];
    assert!(capture_selection(&app.session).is_err());
    Ok(())
}
#[test]
fn settings_json_never_contains_api_keys() -> Result<(), String> {
    let mut settings = Settings::default();
    let p = settings.profiles.first_mut().ok_or("preset")?;
    p.api_key = "test-only-secret-never-serialize".into();
    p.remember_key = true;
    let text = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    assert!(!text.contains("test-only-secret") && !text.contains("apiKey"));
    let loaded: Settings = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert!(loaded.selected()?.api_key.is_empty());
    assert_eq!(loaded.profiles.len(), 6);
    Ok(())
}
#[test]
fn request_owns_selection_and_provider_and_never_applies_answers() -> Result<(), String> {
    let mut app = fixture()?;
    let seen = Arc::new(Mutex::new(None));
    let captured = seen.clone();
    app.services.ai_complete = Some(Arc::new(move |p, m, _| {
        *captured.lock().map_err(|e| e.to_string())? = Some((p.model, m));
        Ok("A1 的板厚为 1.6mm。".into())
    }));
    app.ask_ai("板厚是多少？")?;
    app.run("cell.set", json!({"cell":"A1","input":"后来编辑"}))?;
    app.run("selection.set", json!({"range":"B1"}))?;
    app.ai.settings.profiles.first_mut().ok_or("preset")?.model = "later-model".into();
    let ctx = egui::Context::default();
    for _ in 0..1000 {
        app.poll_ai(&ctx);
        if app.ai.job.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(app.ai.job.is_none());
    let lock = seen.lock().map_err(|e| e.to_string())?;
    let (model, messages) = lock.as_ref().ok_or("capture")?;
    assert_eq!(model, "gpt-4.1-mini");
    let body = &messages.last().ok_or("message")?.content;
    assert!(body.contains("板厚1.6mm") && !body.contains("后来编辑") && !body.contains("不在选区"));
    assert_eq!(app.ai.turns.last().and_then(|t| t.answer.as_deref()), Some("A1 的板厚为 1.6mm。"));
    assert_eq!(app.session.documents().len(), 1);
    assert_eq!(app.session.active().ok_or("doc")?.wb.sheet(0).ok_or("sheet")?.value(CellRef::new(0, 0)).as_text(), Some("后来编辑"));
    Ok(())
}
#[test]
fn switching_workbooks_cancels_and_isolates_conversation() -> Result<(), String> {
    let mut app = fixture()?;
    app.services.ai_complete = Some(Arc::new(|_, _, cancel| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        Ok("旧工作簿回答".into())
    }));
    app.ask_ai("检查选区")?;
    app.session.new_workbook();
    app.poll_ai(&egui::Context::default());
    assert!(app.ai.job.is_none() && app.ai.turns.is_empty());
    app.shutdown_ai();
    assert!(app.ai.retired.is_empty());
    Ok(())
}
#[test]
fn shutdown_cancels_and_joins_active_and_retired_workers() -> Result<(), String> {
    let mut app = fixture()?;
    let finished = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = finished.clone();
    app.services.ai_complete = Some(Arc::new(move |_, _, cancel| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        seen.fetch_add(1, Ordering::Relaxed);
        Err("cancelled synthetic request".into())
    }));
    app.ask_ai("first workbook")?;
    app.session.new_workbook();
    app.poll_ai(&egui::Context::default());
    app.run("cell.set", json!({"cell":"A1","input":"second workbook"}))?;
    app.ask_ai("second workbook")?;
    app.shutdown_ai();
    assert!(app.ai.job.is_none() && app.ai.retired.is_empty());
    assert_eq!(finished.load(Ordering::Relaxed), 2);
    Ok(())
}
#[test]
fn assistant_panel_resizes_expands_and_restores_after_minimize() -> Result<(), String> {
    let mut app = fixture()?;
    app.ai.visible = true;
    app.services.ai_complete = Some(Arc::new(|_, _, _| Ok("synthetic answer".into())));
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    let panel = |h: &egui_kittest::Harness<'_, SheetApp>, name| {
        egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(name)).map(|s| s.outer_rect).ok_or("panel state")
    };
    let initial = panel(&h, "ai_assistant_v2")?;
    assert!(initial.width() >= 550.0, "AI starts wider by default");
    if let Ok(dir) = std::env::var("CRAFT_UI_QA_DIR") {
        h.render().map_err(|e| e.to_string())?.save(std::path::Path::new(&dir).join("gridcraft-ai.png")).map_err(|e| e.to_string())?;
    }
    // The exact shared edge is also the grid's inclusive right boundary.
    // Grab on the panel side of the built-in resize affordance.
    let edge = egui::pos2(initial.left() + 3.0, initial.center().y);
    h.hover_at(edge);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal);
    h.drag_at(edge);
    h.run_steps(2);
    h.hover_at(edge - egui::vec2(100.0, 0.0));
    h.run_steps(3);
    h.drop_at(edge - egui::vec2(100.0, 0.0));
    h.run_steps(2);
    let resized = panel(&h, "ai_assistant_v2")?.width();
    assert!(resized > initial.width() + 70.0);
    h.get_by_label("放大").click();
    h.run_steps(3);
    assert!(panel(&h, "ai_expanded_v2")?.width() > resized);
    h.get_by_label("缩小").click();
    h.run_steps(3);
    assert!((panel(&h, "ai_assistant_v2")?.width() - resized).abs() < 2.0);
    h.get_by_label("—").click();
    h.run_steps(3);
    assert!(h.state().ai.minimized);
    h.get_by_label("AI").click();
    h.run_steps(3);
    assert!(!h.state().ai.minimized);
    assert!((panel(&h, "ai_assistant_v2")?.width() - resized).abs() < 2.0);
    Ok(())
}

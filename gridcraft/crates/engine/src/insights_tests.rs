use super::*;

fn fixture() -> Result<Session> {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "range.setValues",
        json!({"range":"A1","values":[["客户","金额","数量"],["甲","0","2"],["乙","10","3"],["甲","20","4"],["乙","","5"]]}),
    )?;
    s.execute("selection.set", json!({"range":"A1:C5"}))?;
    Ok(s)
}
fn plan() -> ChartPlan {
    ChartPlan {
        title: "客户金额对比".into(),
        chart_type: ChartType::Bar,
        x_column: "A".into(),
        y_columns: vec!["B".into()],
        aggregation: Aggregation::Sum,
        sort: Sort::ValueDesc,
        reason: "按客户汇总金额后比较。".into(),
        x_title: "客户".into(),
        y_title: "金额".into(),
    }
}
#[test]
fn profile_preserves_zero_and_missing_and_excludes_unselected_data() -> Result<()> {
    let mut s = fixture()?;
    s.execute("cell.set", json!({"cell":"Z99","input":"not selected private text"}))?;
    let t = capture(&s, true)?;
    let c = t.columns.get(1).ok_or_else(|| err("column"))?;
    assert_eq!(c.numeric_count, 3);
    assert_eq!(c.missing_count, 1);
    assert_eq!(c.sum, Some(30.0));
    assert_eq!(c.mean, Some(10.0));
    assert_eq!(c.median, Some(10.0));
    assert_eq!(c.min, Some(0.0));
    assert_eq!(c.max, Some(20.0));
    let text = serde_json::to_string(&t).map_err(|e| err(e.to_string()))?;
    assert!(!text.contains("private text"));
    let prepared = prepare(&t, &plan())?;
    assert_eq!(prepared.rows, vec![vec![json!("甲"), json!(20.0)], vec![json!("乙"), json!(10.0)]]);
    Ok(())
}
#[test]
fn local_mean_and_numeric_count_are_not_model_values() -> Result<()> {
    let s = fixture()?;
    let t = capture(&s, true)?;
    let mut p = plan();
    p.aggregation = Aggregation::Mean;
    assert_eq!(prepare(&t, &p)?.rows.first().and_then(|r| r.get(1)).and_then(Json::as_f64), Some(10.0));
    p.aggregation = Aggregation::Count;
    p.sort = Sort::Source;
    assert_eq!(prepare(&t, &p)?.rows, vec![vec![json!("甲"), json!(2.0)], vec![json!("乙"), json!(1.0)]]);
    Ok(())
}
#[test]
fn insertion_is_atomic_undoable_and_original_cells_stay_unchanged() -> Result<()> {
    let mut s = fixture()?;
    let original = s.doc()?.wb.sheet(0).ok_or_else(|| err("source"))?.cells.clone();
    let t = capture(&s, true)?;
    let params = t.params(&plan(), true);
    let before = s.doc()?.undo.len();
    let (wb, si, chart) = preview(&s, &params)?;
    assert_eq!(s.doc()?.wb.sheets.len(), 1);
    assert_eq!(s.doc()?.undo.len(), before);
    let expected = gridcraft_chart::resolve(&wb, si, &chart);
    let inserted = s.execute("insert.aiChart", params)?;
    assert_eq!(s.doc()?.wb.sheets.len(), 2);
    assert_eq!(s.doc()?.undo.len(), before + 1);
    let source = s.doc()?.wb.sheet(0).ok_or_else(|| err("source"))?;
    assert_eq!(source.cells.diff(&original).len(), 0);
    let c = source.charts.first().ok_or_else(|| err("chart"))?;
    let actual = gridcraft_chart::resolve(&s.doc()?.wb, 0, c);
    assert_eq!(actual.categories, expected.categories);
    assert_eq!(actual.series.first().map(|s| &s.values), expected.series.first().map(|s| &s.values));
    assert_eq!(inserted.get("dataSheet").and_then(Json::as_str), Some("AI图表1"));
    s.execute("edit.undo", json!({}))?;
    assert_eq!(s.doc()?.wb.sheets.len(), 1);
    assert!(s.doc()?.wb.sheet(0).ok_or_else(|| err("sheet"))?.charts.is_empty());
    s.execute("edit.redo", json!({}))?;
    assert_eq!(s.doc()?.wb.sheets.len(), 2);
    Ok(())
}
#[test]
fn changed_workbook_is_refused_but_changed_selection_does_not_retarget() -> Result<()> {
    let mut s = fixture()?;
    let t = capture(&s, true)?;
    let params = t.params(&plan(), true);
    s.execute("selection.set", json!({"range":"Z1"}))?;
    s.execute("insert.aiChart", params.clone())?;
    s.execute("edit.undo", json!({}))?;
    assert!(s.execute("insert.aiChart", params).is_err());
    let t = capture_range(&s, 0, RangeRef::parse("A1:C5").ok_or_else(|| err("range"))?, true)?;
    s.execute("cell.set", json!({"cell":"B2","input":"100"}))?;
    assert!(s.execute("insert.aiChart", t.params(&plan(), true)).is_err());
    Ok(())
}
#[test]
fn chart_plan_cannot_inject_refs_formulas_commands_or_values() -> Result<()> {
    let s = fixture()?;
    let t = capture(&s, true)?;
    let mut p = plan();
    p.x_column = "OtherSheet!A".into();
    assert!(prepare(&t, &p).is_err());
    let mut p = plan();
    p.y_columns = vec!["Z".into()];
    assert!(prepare(&t, &p).is_err());
    let mut p = plan();
    p.y_columns = vec!["A".into()];
    assert!(prepare(&t, &p).is_err());
    let mut p = plan();
    p.y_columns = vec!["B".into(), "B".into()];
    assert!(prepare(&t, &p).is_err());
    let mut raw = serde_json::to_value(plan()).map_err(|e| err(e.to_string()))?;
    raw["values"] = json!([999]);
    assert!(serde_json::from_value::<ChartPlan>(raw).is_err());
    let mut raw = serde_json::to_value(plan()).map_err(|e| err(e.to_string()))?;
    raw["chartType"] = json!("execute");
    assert!(serde_json::from_value::<ChartPlan>(raw).is_err());
    Ok(())
}
#[test]
fn malicious_labels_and_titles_are_written_as_text_not_formulas() -> Result<()> {
    let mut s = fixture()?;
    Arc::make_mut(&mut s.doc_mut()?.wb)
        .sheet_mut(0)
        .ok_or_else(|| err("sheet"))?
        .set_value(CellRef::new(1, 0), Value::text("=HYPERLINK(\"https://invalid.test\")"));
    let t = capture(&s, true)?;
    let mut p = plan();
    p.title = "=1+1".into();
    s.execute("insert.aiChart", t.params(&p, false))?;
    let sh = s.doc()?.wb.sheet(1).ok_or_else(|| err("result"))?;
    assert_eq!(sh.value(CellRef::new(0, 0)).as_text(), Some("=1+1"));
    assert!(sh.cells.iter().all(|(_, c)| c.formula.is_none()));
    Ok(())
}
#[test]
fn text_numbers_errors_dates_and_merged_cells_are_not_silently_zeroed() -> Result<()> {
    let mut s = fixture()?;
    s.execute("cell.set", json!({"cell":"B2","input":"'0012"}))?;
    let t = capture(&s, true)?;
    assert!(prepare(&t, &plan()).is_err());
    assert_eq!(t.columns.get(1).map(|c| c.text_count), Some(1));
    s.execute("cell.set", json!({"cell":"B2","input":"=1/0"}))?;
    let t = capture(&s, true)?;
    assert_eq!(t.columns.get(1).map(|c| c.error_count), Some(1));
    assert!(prepare(&t, &plan()).is_err());
    Arc::make_mut(&mut s.doc_mut()?.wb).sheet_mut(0).ok_or_else(|| err("sheet"))?.merges.push(RangeRef::parse("A1:C1").ok_or_else(|| err("range"))?);
    assert!(capture(&s, true).is_err());
    Ok(())
}
#[test]
fn unsafe_pie_and_scatter_are_refused() -> Result<()> {
    let s = fixture()?;
    let t = capture(&s, true)?;
    let mut p = plan();
    p.chart_type = ChartType::Scatter;
    assert!(prepare(&t, &p).is_err());
    p.chart_type = ChartType::Pie;
    p.aggregation = Aggregation::None;
    assert!(prepare(&t, &p).is_err());
    let mut t = t.clone();
    if let Some(cell) = t.rows.first_mut().and_then(|r| r.get_mut(1)) {
        cell.number = Some(-50.0);
    }
    p.aggregation = Aggregation::Sum;
    assert!(prepare(&t, &p).is_err());
    Ok(())
}
#[test]
fn numeric_scatter_pairs_and_sorting_are_preserved() -> Result<()> {
    let mut s = fixture()?;
    s.execute("selection.set", json!({"range":"B1:C4"}))?;
    let t = capture(&s, true)?;
    let mut p = plan();
    p.chart_type = ChartType::Scatter;
    p.x_column = "B".into();
    p.y_columns = vec!["C".into()];
    p.aggregation = Aggregation::None;
    p.sort = Sort::Source;
    assert_eq!(prepare(&t, &p)?.rows, vec![vec![json!(0.0), json!(2.0)], vec![json!(10.0), json!(3.0)], vec![json!(20.0), json!(4.0)]]);
    Ok(())
}
#[test]
fn whole_column_limits_headers_and_protection_are_explicit() -> Result<()> {
    let mut s = fixture()?;
    s.execute("selection.set", json!({"range":"A:C"}))?;
    assert!(capture(&s, true).is_err());
    s.execute("selection.set", json!({"range":"A1:C1"}))?;
    assert!(capture(&s, true).is_err());
    assert!(capture(&s, false).is_ok());
    s.execute("selection.set", json!({"range":"A1:C5"}))?;
    let t = capture(&s, true)?;
    Arc::make_mut(&mut s.doc_mut()?.wb).protected_structure = true;
    assert!(s.execute("insert.aiChart", t.params(&plan(), false)).is_err());
    assert_eq!(s.doc()?.wb.sheets.len(), 1);
    Ok(())
}
#[test]
fn generated_native_charts_survive_xlsx_roundtrip() -> Result<()> {
    let mut s = fixture()?;
    let t = capture(&s, true)?;
    s.execute("insert.aiChart", t.params(&plan(), true))?;
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc()?.wb).map_err(|e| err(e.to_string()))?;
    let (loaded, _) = gridcraft_xlsx::read_xlsx(&bytes).map_err(|e| err(e.to_string()))?;
    let sh = loaded.sheet(0).ok_or_else(|| err("source"))?;
    let chart = sh.charts.first().ok_or_else(|| err("chart"))?;
    let resolved = gridcraft_chart::resolve(&loaded, 0, chart);
    assert_eq!(resolved.categories, vec!["甲", "乙"]);
    assert_eq!(resolved.series.first().map(|s| &s.values), Some(&vec![Some(20.0), Some(10.0)]));
    Ok(())
}

#[test]
fn dates_and_empty_formula_results_have_explicit_statistics() -> Result<()> {
    let mut s = fixture()?;
    s.execute("cell.set", json!({"cell":"B2","input":"46000"}))?;
    s.execute("home.numberFormat", json!({"range":"B2","code":"yyyy-mm-dd"}))?;
    s.execute("cell.set", json!({"cell":"B3","input":"=\"\""}))?;
    let t = capture(&s, true)?;
    let col = t.columns.get(1).ok_or_else(|| err("column"))?;
    assert_eq!(col.date_count, 1);
    assert_eq!(col.missing_count, 2);
    assert_eq!(col.numeric_count, 1);
    assert_eq!(col.sum, Some(20.0));
    assert!(prepare(&t, &plan()).is_err());
    Ok(())
}

#[test]
fn all_supported_chart_types_roundtrip_with_the_same_numeric_data() -> Result<()> {
    for kind in [ChartType::Column, ChartType::Bar, ChartType::Line, ChartType::Pie, ChartType::Doughnut, ChartType::Scatter] {
        let mut s = fixture()?;
        let mut p = plan();
        p.chart_type = kind;
        if kind == ChartType::Scatter {
            s.execute("selection.set", json!({"range":"B1:C4"}))?;
            p.x_column = "B".into();
            p.y_columns = vec!["C".into()];
            p.aggregation = Aggregation::None;
            p.sort = Sort::Source;
        }
        let t = capture(&s, true)?;
        s.execute("insert.aiChart", t.params(&p, false))?;
        let si = s.doc()?.wb.active_sheet;
        let chart = s.doc()?.wb.sheet(si).and_then(|s| s.charts.first()).ok_or_else(|| err("chart"))?;
        let expected = gridcraft_chart::resolve(&s.doc()?.wb, si, chart);
        let bytes = gridcraft_xlsx::write_xlsx(&s.doc()?.wb).map_err(|e| err(e.to_string()))?;
        let (wb, _) = gridcraft_xlsx::read_xlsx(&bytes).map_err(|e| err(e.to_string()))?;
        let loaded = wb.sheet(si).and_then(|s| s.charts.first()).ok_or_else(|| err("reopened chart"))?;
        assert_eq!(loaded.kind, chart.kind);
        let actual = gridcraft_chart::resolve(&wb, si, loaded);
        assert_eq!(actual.series.first().map(|s| &s.values), expected.series.first().map(|s| &s.values));
        assert_eq!(actual.series.first().and_then(|s| s.x.as_ref()), expected.series.first().and_then(|s| s.x.as_ref()));
    }
    Ok(())
}

#[test]
fn finite_overflow_and_missing_only_groups_are_handled_without_invented_zeroes() -> Result<()> {
    let mut s = fixture()?;
    s.execute("cell.set", json!({"cell":"B3","input":""}))?;
    let t = capture(&s, true)?;
    let mut p = plan();
    p.aggregation = Aggregation::Count;
    assert!(prepare(&t, &p)?.rows.iter().any(|r| r.first().and_then(Json::as_str) == Some("乙") && r.get(1).and_then(Json::as_f64) == Some(0.0)));
    p.aggregation = Aggregation::Sum;
    assert!(prepare(&t, &p)?.rows.iter().any(|r| r.first().and_then(Json::as_str) == Some("乙") && r.get(1).is_some_and(Json::is_null)));
    s.execute("cell.set", json!({"cell":"B2","input":"1e308"}))?;
    s.execute("cell.set", json!({"cell":"B4","input":"1e308"}))?;
    let t = capture(&s, true)?;
    assert_eq!(t.columns.get(1).and_then(|s| s.sum), None);
    assert!(prepare(&t, &p).is_err());
    Ok(())
}

#[test]
fn helper_preserves_uniform_percent_format_but_count_is_plain() -> Result<()> {
    let mut s = fixture()?;
    s.execute("home.numberFormat", json!({"range":"B2:B5","code":"0.0%"}))?;
    let t = capture(&s, true)?;
    s.execute("insert.aiChart", t.params(&plan(), false))?;
    let wb = &s.doc()?.wb;
    let sh = wb.sheet(1).ok_or_else(|| err("helper"))?;
    assert_eq!(wb.styles.get(sh.style_id(CellRef::new(5, 1))).num_fmt.as_str(), "0.0%");
    assert_eq!(sh.value(CellRef::new(5, 1)), Value::Number(20.0));
    assert_eq!(s.doc()?.selection.active, CellRef::new(0, 0));
    s.execute("edit.undo", json!({}))?;
    let t = capture(&s, true)?;
    let mut p = plan();
    p.aggregation = Aggregation::Count;
    s.execute("insert.aiChart", t.params(&p, false))?;
    let wb = &s.doc()?.wb;
    let sh = wb.sheet(1).ok_or_else(|| err("helper"))?;
    assert_eq!(wb.styles.get(sh.style_id(CellRef::new(5, 1))).num_fmt.as_str(), "General");
    Ok(())
}

use crate::{read_xlsx, write_xlsx};
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, RichTextRun, RunFont, Workbook};

#[test]
fn shared_and_inline_rich_text_keep_inheritance_and_character_styles() {
    let sst = r#"<sst><si><r><rPr><b/></rPr><t xml:space="preserve"> 中文 </t></r><r><rPr><rFont val="Times New Roman"/><sz val="20"/><color rgb="FFFF0000"/><i val="0"/></rPr><t>ABC</t></r></si></sst>"#;
    let body = r#"<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="inlineStr"><is><r><rPr><sz val="9"/></rPr><t xml:space="preserve"> 中文 ABC</t></r></is></c></row></sheetData>"#;
    let rels = r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#;
    let bytes = super::minimal(body, &[("xl/sharedStrings.xml", sst)], rels, "");
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let a = CellRef::parse("A1").unwrap();
    let b = CellRef::parse("B1").unwrap();
    let cell = wb.active().unwrap().cells.get(a).unwrap();
    assert_eq!(cell.value, Value::text(" 中文 ABC"));
    let runs = cell.rich_runs().unwrap();
    assert_eq!(runs[0].font.bold, Some(true));
    assert_eq!(runs[0].font.name, None);
    assert_eq!(runs[0].font.size, None);
    assert_eq!(runs[1].font.size, Some(20.0));
    assert_eq!(runs[1].font.italic, Some(false));
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for at in [a, b] {
        assert_eq!(again.active().unwrap().cells.get(at).unwrap().rich_runs(), wb.active().unwrap().cells.get(at).unwrap().rich_runs());
    }
}

#[test]
fn identical_text_with_different_run_formats_stays_distinct_and_stale_runs_are_ignored() {
    let mut wb = Workbook::new();
    for (col, size) in [(0, 9.0), (1, 20.0)] {
        wb.sheet_mut(0).unwrap().set_cell(
            CellRef::new(0, col),
            Cell {
                value: Value::text("中文 ABC"),
                rich_text: vec![RichTextRun { text: "中文 ABC".into(), font: RunFont { size: Some(size), ..Default::default() } }],
                ..Default::default()
            },
        );
    }
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for (col, size) in [(0, 9.0), (1, 20.0)] {
        assert_eq!(again.active().unwrap().cells.get(CellRef::new(0, col)).unwrap().rich_text[0].font.size, Some(size));
    }
    let mut cell = again.active().unwrap().cells.get(CellRef::new(0, 0)).unwrap().clone();
    cell.value = Value::text("changed");
    assert!(cell.rich_runs().is_none());
}

#[test]
fn rich_text_and_phonetic_hints_both_survive_inline_export() {
    let body = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><r><rPr><b/></rPr><t>東</t></r><rPh sb="0" eb="1"><t>ヒガシ</t></rPh><phoneticPr fontId="0"/></is></c></row></sheetData>"#;
    let (wb, _) = read_xlsx(&super::minimal(body, &[], "", "")).unwrap();
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(again.phonetics, wb.phonetics);
    assert_eq!(again.phonetics.len(), 1);
    assert_eq!(
        again.active().unwrap().cells.get(CellRef::new(0, 0)).unwrap().rich_runs(),
        wb.active().unwrap().cells.get(CellRef::new(0, 0)).unwrap().rich_runs()
    );
}

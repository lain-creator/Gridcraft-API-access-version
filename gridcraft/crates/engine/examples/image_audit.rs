//! Local diagnostic: inspect pictures and round-trip in memory; never writes the source workbook.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let input = args.get(1).ok_or("workbook required")?;
    let mut session = gridcraft_engine::Session::new();
    session.execute("file.open", serde_json::json!({"path":input}))?;
    let source = &session.active().ok_or("no workbook")?.wb;
    let bytes = gridcraft_xlsx::write_xlsx(source)?;
    let (restored, _) = gridcraft_xlsx::read_xlsx(&bytes)?;
    let mut count = 0;
    for (a, b) in source.sheets.iter().zip(&restored.sheets) {
        if a.images.len() != b.images.len() {
            return Err("image count changed".into());
        }
        for (x, y) in a.images.iter().zip(&b.images) {
            if x.data != y.data || x.mime != y.mime {
                return Err("image content changed".into());
            }
            if x.crop.iter().zip(y.crop).any(|(x, y)| (x - y).abs() > 1e-6) {
                return Err("image crop changed".into());
            }
            count += 1;
        }
    }
    println!("{count} pictures retained, bytes and crop unchanged after XLSX round-trip");
    if let Some(output) = args.get(2) {
        session.execute("file.exportPdf", serde_json::json!({"path":output}))?;
        println!("PDF exported for visual inspection");
    }
    Ok(())
}

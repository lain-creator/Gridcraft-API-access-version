fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let bytes = std::fs::read(args.get(1).ok_or("input required")?)?;
    let picture = gridcraft_metafile::parse(&bytes)?;
    println!("metafile {}x{}, {} items", picture.width, picture.height, picture.items.len());
    let (rgba, w, h) = gridcraft_metafile::rasterize(&bytes, 2048, 2048).ok_or("raster failed")?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("allocation failed")?;
    for (p, s) in pixmap.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
        for i in 0..3 {
            p[i] = ((u16::from(s[i]) * u16::from(s[3]) + 127) / 255) as u8;
        }
        p[3] = s[3];
    }
    pixmap.save_png(args.get(2).ok_or("output required")?)?;
    Ok(())
}

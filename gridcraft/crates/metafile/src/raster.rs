//! Bounded raster fallback for spreadsheet metafiles. Original bytes stay in the workbook.
use std::sync::OnceLock;

use crate::{Item, PlacedItem};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

fn transform(a: kurbo::Affine) -> Transform {
    let [a, b, c, d, e, f] = a.as_coeffs();
    Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
}

fn paint(color: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(color[0], color[1], color[2], color[3]);
    p
}

fn path(src: &kurbo::BezPath) -> Option<tiny_skia::Path> {
    let mut p = PathBuilder::new();
    for el in src.elements() {
        match el {
            kurbo::PathEl::MoveTo(a) => p.move_to(a.x as f32, a.y as f32),
            kurbo::PathEl::LineTo(a) => p.line_to(a.x as f32, a.y as f32),
            kurbo::PathEl::QuadTo(a, b) => p.quad_to(a.x as f32, a.y as f32, b.x as f32, b.y as f32),
            kurbo::PathEl::CurveTo(a, b, c) => p.cubic_to(a.x as f32, a.y as f32, b.x as f32, b.y as f32, c.x as f32, c.y as f32),
            kurbo::PathEl::ClosePath => p.close(),
        }
    }
    p.finish()
}

struct Outline(PathBuilder);
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

fn fonts() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        #[allow(unused_mut)]
        let mut db = fontdb::Database::new();
        #[cfg(not(target_arch = "wasm32"))]
        db.load_system_fonts();
        db
    })
}

fn draw_text(dst: &mut Pixmap, text: &crate::Text, tf: kurbo::Affine) {
    let db = fonts();
    let families = [fontdb::Family::Name(&text.font), fontdb::Family::SansSerif];
    let query = fontdb::Query {
        families: &families,
        weight: if text.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
        style: if text.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        ..Default::default()
    };
    let Some(id) = db.query(&query).or_else(|| db.faces().next().map(|f| f.id)) else { return };
    db.with_face_data(id, |data, index| {
        let Ok(face) = ttf_parser::Face::parse(data, index) else { return };
        let upem = f64::from(face.units_per_em());
        let glyphs: Vec<_> = text
            .text
            .char_indices()
            .map(|(i, c)| {
                let glyph = face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0));
                (i, f64::from(face.glyph_hor_advance(glyph).unwrap_or(0)), 0.0, 0.0)
            })
            .collect();
        let Some(layout) = text.layout(upem, f64::from(face.ascender()), -f64::from(face.descender()), &glyphs) else { return };
        let k = layout.em / upem;
        for (c, &(x, y)) in text.text.chars().zip(&layout.glyphs) {
            let Some(glyph) = face.glyph_index(c) else { continue };
            let mut outline = Outline(PathBuilder::new());
            face.outline_glyph(glyph, &mut outline);
            if let Some(path) = outline.0.finish() {
                let at = tf * kurbo::Affine::translate((x, y)) * kurbo::Affine::scale_non_uniform(k, -k);
                dst.fill_path(&path, &paint(text.color), FillRule::Winding, transform(at), None);
            }
        }
    });
}

/// Returns straight-alpha RGBA. Limits apply before decoding and before allocating a canvas.
pub fn rasterize(data: &[u8], max_w: u32, max_h: u32) -> Option<(Vec<u8>, u32, u32)> {
    if data.len() > 64 * 1024 * 1024 {
        return None;
    }
    let picture = crate::parse(data).ok()?;
    if !picture.has_size() {
        return None;
    }
    let k = (max_w.clamp(1, 4096) as f32 / picture.width).min(max_h.clamp(1, 4096) as f32 / picture.height);
    let w = (picture.width * k).round().clamp(1.0, 4096.0) as u32;
    let h = (picture.height * k).round().clamp(1.0, 4096.0) as u32;
    let mut dst = Pixmap::new(w, h)?;
    for item in picture.place(kurbo::Rect::new(0.0, 0.0, w as f64, h as f64), [0.0; 4]) {
        match item {
            PlacedItem::Path { path: src, fill, stroke, even_odd } => {
                let Some(path) = path(&src) else { continue };
                if let Some(color) = fill {
                    dst.fill_path(&path, &paint(color), if even_odd { FillRule::EvenOdd } else { FillRule::Winding }, Transform::identity(), None);
                }
                if let Some((color, width)) = stroke {
                    let stroke = Stroke {
                        width: (width as f32).max(1.0),
                        line_cap: tiny_skia::LineCap::Round,
                        line_join: tiny_skia::LineJoin::Round,
                        ..Default::default()
                    };
                    dst.stroke_path(&path, &paint(color), &stroke, Transform::identity(), None);
                }
            }
            PlacedItem::Bitmap { index, rect } => {
                let Some(Item::Bitmap { width, height, rgba, .. }) = picture.items.get(index) else { continue };
                let Some(mut source) = Pixmap::new(*width, *height) else { continue };
                if source.data().len() != rgba.len() {
                    continue;
                }
                for (out, pixel) in source.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
                    for i in 0..3 {
                        out[i] = ((u16::from(pixel[i]) * u16::from(pixel[3]) + 127) / 255) as u8;
                    }
                    out[3] = pixel[3];
                }
                let tf = Transform::from_row(
                    (rect.width() / f64::from(*width)) as f32,
                    0.0,
                    0.0,
                    (rect.height() / f64::from(*height)) as f32,
                    rect.x0 as f32,
                    rect.y0 as f32,
                );
                dst.draw_pixmap(
                    0,
                    0,
                    source.as_ref(),
                    &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() },
                    tf,
                    None,
                );
            }
            PlacedItem::Text { index, tf } => {
                if let Some(Item::Text(text)) = picture.items.get(index) {
                    draw_text(&mut dst, text, tf);
                }
            }
        }
    }
    let mut rgba = dst.take();
    for p in rgba.chunks_exact_mut(4) {
        if p[3] > 0 {
            for i in 0..3 {
                p[i] = ((u16::from(p[i]) * 255 + u16::from(p[3]) / 2) / u16::from(p[3])).min(255) as u8;
            }
        }
    }
    Some((rgba, w, h))
}

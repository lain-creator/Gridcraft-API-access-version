//! Floating pictures on the sheet, with original synthetic pictures.

use std::io::Cursor;

use egui::{Shape, TextureId, vec2};
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

fn png(color: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(120, 60, image::Rgba(color));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

fn harness() -> egui_kittest::Harness<'static, SheetApp> {
    let mut session = Session::new();
    session.new_workbook();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        SheetApp::new(session, Default::default()),
    );
    h.run_steps(4);
    h
}

fn picture_textures(h: &egui_kittest::Harness<'static, SheetApp>) -> Vec<TextureId> {
    h.output()
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Mesh(mesh) if mesh.texture_id != TextureId::default() => Some(mesh.texture_id),
            _ => None,
        })
        .collect()
}

/// Picture ids are per workbook, so two open workbooks both have a picture 1. Each must show its
/// own picture, even when the two are the same number of bytes: the texture cache used to tell
/// pictures apart by id and size alone, for the whole app.
#[test]
fn each_workbook_shows_its_own_picture() {
    let (green, red) = (png([24, 150, 110, 255]), png([200, 40, 40, 255]));
    assert_eq!(green.len(), red.len());
    let mut h = harness();
    h.state_mut().run("insert.picture", json!({"base64": gridcraft_engine::io::base64_encode(&green), "at": "B2"})).unwrap();
    h.run_steps(4);
    let first = picture_textures(&h);
    assert_eq!(first.len(), 1, "the picture is painted");

    h.state_mut().run("file.new", json!({})).unwrap();
    h.state_mut().run("insert.picture", json!({"base64": gridcraft_engine::io::base64_encode(&red), "at": "B2"})).unwrap();
    h.run_steps(4);
    let doc = h.state().session.active().unwrap();
    assert_eq!(doc.wb.active().unwrap().images[0].id, h.state().session.documents()[0].wb.active().unwrap().images[0].id);
    let second = picture_textures(&h);
    assert_eq!(second.len(), 1);
    assert_ne!(second, first, "another workbook's picture 1 has a texture of its own");

    // Closing the workbook and painting again leaves the first one's picture as it was.
    h.state_mut().run("file.close", json!({"force": true})).unwrap();
    h.run_steps(4);
    assert_eq!(picture_textures(&h), first);
}

#[test]
fn floating_picture_uses_saved_crop_coordinates() {
    let mut h = harness();
    h.state_mut().run("insert.picture", json!({"base64": gridcraft_engine::io::base64_encode(&png([20,100,200,255]))})).unwrap();
    let doc = h.state_mut().session.active_mut().unwrap();
    let sheet = doc.wb.active_sheet;
    std::sync::Arc::make_mut(&mut doc.wb).sheet_mut(sheet).unwrap().images[0].crop = [0.25, 0.1, 0.25, 0.2];
    h.run_steps(3);
    let mesh = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            Shape::Mesh(m) if m.texture_id != TextureId::default() => Some(m),
            _ => None,
        })
        .unwrap();
    let (min_u, max_u, min_v, max_v) =
        mesh.vertices.iter().fold((1.0f32, 0.0f32, 1.0f32, 0.0f32), |(a, b, c, d), v| (a.min(v.uv.x), b.max(v.uv.x), c.min(v.uv.y), d.max(v.uv.y)));
    assert!((min_u - 0.25).abs() < 1e-6 && (max_u - 0.75).abs() < 1e-6 && (min_v - 0.1).abs() < 1e-6 && (max_v - 0.8).abs() < 1e-6);
}

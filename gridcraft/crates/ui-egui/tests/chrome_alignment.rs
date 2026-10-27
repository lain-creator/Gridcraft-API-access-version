//! Control contents must be centered independently of the surrounding flow direction.

use egui::{Align, Color32, FontFamily, FontId, Layout, Rect, Shape, pos2, vec2};
use gridcraft_ui_egui::{i18n::Language, theme};

#[test]
fn button_labels_are_centered_in_every_parent_layout() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for layout in [Layout::top_down(Align::Min), Layout::left_to_right(Align::Min), Layout::right_to_left(Align::Center)] {
            for label in ["API 设置", "AI 选区问答", "翻译选中区域", "Translate", "保存设置"] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                ctx.set_fonts(theme::font_definitions_for_language(Language::Zh));
                theme::apply(&ctx, false);
                let mut rect = Rect::NOTHING;
                let mut output = ctx.run_ui(
                    egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 140.0))), ..Default::default() },
                    |ui| {
                        ui.with_layout(layout, |ui| {
                            rect = ui.add(egui::Button::new(label).min_size(vec2(158.0, 28.0))).rect;
                        });
                    },
                );
                output.textures_delta.clear();
                let text = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        Shape::Text(t) if t.galley.job.text == label => Some(t),
                        _ => None,
                    })
                    .expect("button paints its complete label");
                let text_rect = text.galley.rect.translate(text.pos.to_vec2());
                let ink = text.galley.mesh_bounds.translate(text.pos.to_vec2());
                assert!(rect.contains_rect(ink), "{label}: clipped ink {ink:?}, button {rect:?}, scale {scale}");
                assert!(
                    (text_rect.center() - rect.center()).length() <= 1.5 / scale,
                    "{label}: text {text_rect:?}, button {rect:?}, parent {layout:?}, scale {scale}"
                );
            }
        }
    }
}

#[test]
fn mixed_script_ui_font_has_balanced_visible_bounds() {
    let ctx = egui::Context::default();
    ctx.set_fonts(theme::font_definitions_for_language(Language::Zh));
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {});
    output.textures_delta.clear();
    for size in [11.0, 12.5, 13.0, 18.0] {
        for family in [theme::UI, theme::UI_BOLD] {
            let font = FontId::new(size, FontFamily::Name(family.into()));
            let samples = ["API", "设置", "API 设置", "AI 选区问答", "English 中文 123"];
            let mut centers = Vec::new();
            for text in samples {
                let galley = ctx.fonts_mut(|f| f.layout_no_wrap(text.into(), font.clone(), Color32::BLACK));
                centers.push(galley.mesh_bounds.center().y);
                assert!(
                    (galley.mesh_bounds.center().y - galley.rect.center().y).abs() <= 2.0,
                    "{family} {size} {text}: visible text must be vertically centered in its line"
                );
            }
            let min = centers.iter().copied().fold(f32::INFINITY, f32::min);
            let max = centers.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(max - min <= 2.0, "{family} {size}: Latin and Chinese ink centers must agree: {centers:?}");
        }
    }
}

#[test]
fn single_line_input_centers_text_and_caret_but_multiline_stays_top_aligned() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(scale);
        ctx.set_fonts(theme::font_definitions_for_language(Language::Zh));
        theme::apply(&ctx, false);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut single = "API 输入位置 abc123".to_owned();
            let edit = egui::TextEdit::singleline(&mut single).min_size(vec2(260.0, 36.0)).show(ui);
            let line = edit.galley.rect.translate(edit.galley_pos.to_vec2());
            let ink = edit.galley.mesh_bounds.translate(edit.galley_pos.to_vec2());
            assert!((line.center().y - edit.text_clip_rect.center().y).abs() <= 1.0 / scale);
            assert!(edit.response.rect.contains_rect(ink));
            // Cursor hit testing/painting share galley_pos, not an independently shifted label.
            let caret = edit.galley.pos_from_cursor(egui::text::CCursor::new(4)).translate(edit.galley_pos.to_vec2());
            assert!((caret.center().y - line.center().y).abs() <= 1.0 / scale);
            let mut multi = "第一行中文 English\n第二行".to_owned();
            let edit = egui::TextEdit::multiline(&mut multi).min_size(vec2(260.0, 100.0)).show(ui);
            assert!((edit.galley_pos.y - edit.text_clip_rect.top()).abs() <= 1.0 / scale);
        });
        output.textures_delta.clear();
    }
}

#[test]
fn centered_buttons_preserve_menu_shortcut_separation() {
    let ctx = egui::Context::default();
    ctx.set_fonts(theme::font_definitions_for_language(Language::Zh));
    theme::apply(&ctx, false);
    let mut rect = Rect::NOTHING;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        rect = ui.add(egui::Button::new("保存副本").shortcut_text("Ctrl+Shift+S").min_size(vec2(280.0, 28.0))).rect;
    });
    output.textures_delta.clear();
    let text_bounds = |label| {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Text(t) if t.galley.job.text == label => Some(t.galley.rect.translate(t.pos.to_vec2())),
                _ => None,
            })
            .expect("menu text")
    };
    let label = text_bounds("保存副本");
    let shortcut = text_bounds("Ctrl+Shift+S");
    assert!(label.left() - rect.left() <= 10.0);
    assert!(rect.right() - shortcut.right() <= 10.0);
    assert!(shortcut.left() - label.right() > 20.0);
}

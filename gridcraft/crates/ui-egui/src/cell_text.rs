//! One font layout path for worksheet drawing and measured automatic row heights.
use egui::{
    Color32, FontId, Stroke,
    text::{LayoutJob, TextFormat},
};
use gridcraft_engine::model::{Cell, Font, Theme, Underline, VertAlign};

pub fn job(cell: Option<&Cell>, text: &str, base: &Font, scale: f32, color: Color32, width: f32, wrap: bool, theme: &Theme) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = if wrap { width.max(1.0) } else { f32::INFINITY };
    let format = |font: &Font, color: Color32| {
        let size = font.size * 96.0 / 72.0 * scale;
        TextFormat {
            font_id: FontId::new(size, crate::theme::cell_family(&font.name, font.bold, font.italic)),
            color,
            underline: if font.underline == Underline::None { Stroke::NONE } else { Stroke::new(1.0 * scale, color) },
            strikethrough: if font.strike { Stroke::new(1.0 * scale, color) } else { Stroke::NONE },
            valign: match font.vert {
                VertAlign::Superscript => egui::Align::TOP,
                VertAlign::Subscript => egui::Align::BOTTOM,
                _ => egui::Align::BOTTOM,
            },
            ..Default::default()
        }
    };
    if let Some(runs) = cell.and_then(Cell::rich_runs) {
        for run in runs {
            let text = if wrap { run.text.clone() } else { run.text.replace(['\r', '\n'], "") };
            let color = run.font.color.and_then(|c| c.resolve(theme)).map(|[r, g, b]| Color32::from_rgb(r, g, b)).unwrap_or(color);
            job.append(&text, 0.0, format(&run.font.resolve(base), color));
        }
    } else {
        job.append(text, 0.0, format(base, color));
    }
    job
}

pub fn height(ctx: &egui::Context, cell: &Cell, font: &Font, width: Option<f32>) -> Option<f32> {
    let text = match &cell.value {
        gridcraft_engine::core::Value::Text(t) => t.to_string(),
        value => value.display(),
    };
    let text = if width.is_some() { text } else { text.replace(['\r', '\n'], "") };
    let job = job(Some(cell), &text, font, 1.0, Color32::BLACK, width.unwrap_or(f32::INFINITY), width.is_some(), &Theme::default());
    ctx.fonts_mut(|f| {
        if job.sections.iter().any(|s| !f.families().contains(&s.format.font_id.family)) {
            return None;
        }
        let galley = f.layout_job(job);
        Some(galley.rect.union(galley.mesh_bounds).height())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::{
        core::Value,
        model::{RichTextRun, RunFont},
    };

    #[test]
    fn run_inheritance_and_paint_measure_share_one_layout_job() {
        let base = Font { name: "宋体".into(), size: 16.0, ..Default::default() };
        let cell = Cell {
            value: Value::text("中文 ABC"),
            rich_text: vec![
                RichTextRun { text: "中文 ".into(), font: RunFont { bold: Some(true), ..Default::default() } },
                RichTextRun { text: "ABC".into(), font: RunFont { size: Some(24.0), ..Default::default() } },
            ],
            ..Default::default()
        };
        let job = job(Some(&cell), "中文 ABC", &base, 1.0, Color32::BLACK, 120.0, true, &Theme::default());
        assert_eq!(job.text, "中文 ABC");
        assert_eq!(job.sections.len(), 2);
        assert!((job.sections[0].format.font_id.size - 16.0 * 96.0 / 72.0).abs() < 0.01);
        assert_eq!(job.sections[1].format.font_id.size, 32.0);
        assert_eq!(job.wrap.max_width, 120.0);
    }
}

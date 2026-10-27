//! Character formatting in shared and inline XLSX strings (ISO 29500 §18.4).
use crate::xml::{El, encode_xstring, esc, esc_attr};
use gridcraft_model::{RichTextRun, RunFont, Underline, VertAlign};
use std::fmt::Write as _;

pub fn read(e: &El, palette: &[u32]) -> Vec<RichTextRun> {
    if e.kids("r").next().is_none() {
        return Vec::new();
    }
    e.children
        .iter()
        .filter_map(|r| {
            if r.name == "t" {
                return Some(RichTextRun { text: crate::xml::decode_xstring(&r.text), font: RunFont::default() });
            }
            if r.name != "r" {
                return None;
            }
            let text = r.kids("t").map(|t| crate::xml::decode_xstring(&t.text)).collect::<String>();
            let mut font = RunFont::default();
            if let Some(p) = r.child("rPr") {
                let f = crate::styles::read_font(p, palette);
                font.name = p.child("rFont").or_else(|| p.child("name")).and_then(|n| n.attr("val")).map(str::to_string);
                font.size = p.child("sz").and_then(|n| n.attr_f64("val")).filter(|v| v.is_finite()).map(|_| f.size);
                font.bold = p.child("b").map(|_| f.bold);
                font.italic = p.child("i").map(|_| f.italic);
                font.strike = p.child("strike").map(|_| f.strike);
                font.underline = p.child("u").map(|_| f.underline);
                font.color = p.child("color").map(|_| f.color);
                font.vert = p.child("vertAlign").map(|_| f.vert);
                font.charset = f.charset;
                font.family = f.family;
            }
            Some(RichTextRun { text, font })
        })
        .collect()
}

pub fn write(runs: &[RichTextRun]) -> String {
    let mut xml = String::new();
    for r in runs {
        xml.push_str("<r><rPr>");
        let f = &r.font;
        if let Some(name) = &f.name {
            let _ = write!(xml, "<rFont val=\"{}\"/>", esc_attr(name));
        }
        if let Some(size) = f.size.filter(|s| s.is_finite()) {
            let _ = write!(xml, "<sz val=\"{}\"/>", size.clamp(1.0, 409.0));
        }
        for (tag, on) in [("b", f.bold), ("i", f.italic), ("strike", f.strike)] {
            if let Some(on) = on {
                let _ = write!(xml, "<{tag} val=\"{}\"/>", u8::from(on));
            }
        }
        if let Some(color) = f.color {
            let _ = write!(xml, "<color {}/>", crate::styles::color_attrs(&color));
        }
        if let Some(u) = f.underline {
            let u = match u {
                Underline::None => "none",
                Underline::Single => "single",
                Underline::Double => "double",
                Underline::SingleAccounting => "singleAccounting",
                Underline::DoubleAccounting => "doubleAccounting",
            };
            let _ = write!(xml, "<u val=\"{u}\"/>");
        }
        if let Some(v) = f.vert {
            let v = match v {
                VertAlign::Baseline => "baseline",
                VertAlign::Superscript => "superscript",
                VertAlign::Subscript => "subscript",
            };
            let _ = write!(xml, "<vertAlign val=\"{v}\"/>");
        }
        if let Some(c) = f.charset {
            let _ = write!(xml, "<charset val=\"{c}\"/>");
        }
        if let Some(c) = f.family {
            let _ = write!(xml, "<family val=\"{c}\"/>");
        }
        let _ = write!(xml, "</rPr><t xml:space=\"preserve\">{}</t></r>", esc(&encode_xstring(&r.text)));
    }
    xml
}

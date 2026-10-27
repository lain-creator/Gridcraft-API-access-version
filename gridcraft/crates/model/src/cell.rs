//! Cells.

use std::sync::Arc;

use gridcraft_core::{RangeRef, Value};
use gridcraft_formula::Expr;
use serde::{Deserialize, Serialize};

use crate::style::StyleId;
use crate::{Color, Font, Underline, VertAlign};

/// Only the properties explicitly stated by an XLSX rich-text run. Missing properties inherit
/// the cell font, so a bold-only run must not accidentally reset a 16pt Chinese cell to Calibri.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunFont {
    pub name: Option<String>,
    pub size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    pub color: Option<Color>,
    pub vert: Option<VertAlign>,
    pub charset: Option<u8>,
    pub family: Option<u8>,
}

impl RunFont {
    /// Apply only properties changed by a whole-cell font command; keep unrelated inheritance.
    pub fn update(&mut self, before: &Font, after: &Font) {
        macro_rules! property {
            ($($p:ident),*) => {$(if before.$p != after.$p {self.$p = Some(after.$p.clone());})*};
        }
        property!(name, size, bold, italic, underline, strike, color, vert);
        if before.charset != after.charset {
            self.charset = after.charset;
        }
        if before.family != after.family {
            self.family = after.family;
        }
    }

    pub fn resolve(&self, base: &Font) -> Font {
        Font {
            name: self.name.clone().unwrap_or_else(|| base.name.clone()),
            size: self.size.unwrap_or(base.size),
            bold: self.bold.unwrap_or(base.bold),
            italic: self.italic.unwrap_or(base.italic),
            underline: self.underline.unwrap_or(base.underline),
            strike: self.strike.unwrap_or(base.strike),
            color: self.color.unwrap_or(base.color),
            vert: self.vert.unwrap_or(base.vert),
            charset: self.charset.or(base.charset),
            family: self.family.or(base.family),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RichTextRun {
    pub text: String,
    pub font: RunFont,
}

/// A parsed formula. `text` is the canonical formula text without the leading `=`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Formula {
    pub text: String,
    #[serde(skip)]
    pub expr: Option<Arc<Expr>>,
    /// Legacy array formula (Ctrl+Shift+Enter) covering this range; the formula lives in the
    /// top-left cell.
    pub array: Option<RangeRef>,
    /// Read from a file that marks this formula as a dynamic-array formula (cell metadata `cm`).
    /// The writer keeps the mark, so a formula Excel evaluates as an array is never saved as a
    /// legacy formula (which Excel evaluates with implicit intersection). Formulas entered or
    /// rewritten in GridCraft start without it, and the writer decides from their shape.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dynamic: bool,
}

impl Formula {
    /// Parses `text` (with or without `=`). Unparseable formulas keep their text and evaluate to
    /// `#NAME?`.
    pub fn new(text: &str) -> Formula {
        let body = text.strip_prefix('=').unwrap_or(text);
        match gridcraft_formula::parse(body) {
            Ok(e) => Formula { text: gridcraft_formula::print(&e), expr: Some(Arc::new(e)), array: None, dynamic: false },
            Err(_) => Formula { text: body.to_string(), expr: None, array: None, dynamic: false },
        }
    }
    pub fn from_expr(e: Expr) -> Formula {
        Formula { text: gridcraft_formula::print(&e), expr: Some(Arc::new(e)), array: None, dynamic: false }
    }
    /// The parsed expression, re-parsing after deserialization.
    pub fn expr(&self) -> Option<Expr> {
        self.expr.as_deref().cloned().or_else(|| gridcraft_formula::parse(&self.text).ok())
    }
    /// The parsed expression without copying it (re-parses after deserialization).
    pub fn expr_arc(&self) -> Option<Arc<Expr>> {
        self.expr.clone().or_else(|| gridcraft_formula::parse(&self.text).ok().map(Arc::new))
    }
}

/// A static picture stored as cell content, independent of floating drawing anchors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellPicture {
    /// Encoded PNG/JPEG bytes, base64 in JSON. Sheet entries share the payload through Arc.
    #[serde(with = "crate::b64")]
    pub data: Vec<u8>,
    pub mime: String,
    pub alt: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// Constant value, or the cached result of the formula.
    pub value: Value,
    pub formula: Option<Arc<Formula>>,
    pub style: StyleId,
    /// Character formatting travels with the cell through copy, sort, structural edits and undo.
    /// Plain/older JSON has no entry. Stale runs are never applied to newly replaced text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rich_text: Vec<RichTextRun>,
}

impl Cell {
    pub fn value(v: Value) -> Cell {
        Cell { value: v, ..Default::default() }
    }
    pub fn formula(f: Formula) -> Cell {
        Cell { formula: Some(Arc::new(f)), ..Default::default() }
    }
    pub fn rich_runs(&self) -> Option<&[RichTextRun]> {
        let Value::Text(text) = &self.value else { return None };
        if self.formula.is_some() || self.rich_text.is_empty() {
            return None;
        }
        let same = self.rich_text.iter().flat_map(|r| r.text.chars()).eq(text.chars());
        same.then_some(self.rich_text.as_slice())
    }
    /// No value, no formula and the default style: storing it is pointless.
    pub fn is_blank(&self) -> bool {
        self.value.is_empty() && self.formula.is_none() && self.style == StyleId::DEFAULT
    }
    /// What the formula bar shows.
    pub fn input_text(&self) -> String {
        if let Some(f) = &self.formula {
            return format!("={}", f.text);
        }
        match &self.value {
            Value::Text(t) => {
                // Text that would parse as something else is shown with its apostrophe by Excel
                // only when entered that way; we show it plainly.
                t.to_string()
            }
            v => v.display(),
        }
    }
    /// What the formula bar shows when not editing: [`input_text`](Self::input_text), with a
    /// legacy array formula in braces (`{=A1:A2*2}`).
    pub fn bar_text(&self) -> String {
        match &self.formula {
            Some(f) if f.array.is_some() => format!("{{={}}}", f.text),
            _ => self.input_text(),
        }
    }
}

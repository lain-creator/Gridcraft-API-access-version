//! Text-only translation planning and insertion into a frozen document copy.
//! Network access belongs to the desktop host. Original runs and objects are never replaced.
use crate::Selection;
use std::collections::HashMap;
use wordcraft_doc::{Document, PartKind, Pos, StoryRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}
impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "英文",
            Self::Chinese => "中文",
        }
    }
    pub fn api(self) -> (&'static str, &'static str) {
        match self {
            Self::English => ("zh-to-target", "English"),
            Self::Chinese => ("auto-to-zh", "Chinese"),
        }
    }
}
#[derive(Clone, Debug)]
pub enum Scope {
    Document,
    Selection(Vec<Selection>),
}
#[derive(Clone)]
pub struct Request {
    pub doc: Document,
    pub scope: Scope,
    pub target: Language,
}
struct Target {
    at: Pos,
    style_at: usize,
    text: usize,
    paragraph_end: bool,
}
pub struct Plan {
    pub texts: Vec<String>,
    targets: Vec<Target>,
    pub skipped_bilingual: usize,
    pub skipped_special: usize,
}
impl Plan {
    pub fn changed(&self) -> usize {
        self.targets.len()
    }
    pub fn occurrences(&self, text: usize) -> usize {
        self.targets.iter().filter(|t| t.text == text).count()
    }
}
pub fn chinese(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x2fa1f | 0x30000..=0x323af))
}
fn english(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_alphabetic())
}
fn eligible(s: &str, target: Language) -> bool {
    match target {
        Language::English => chinese(s),
        Language::Chinese => s.chars().any(|c| c.is_alphabetic() && !chinese(&c.to_string())),
    }
}

pub fn plan(request: &Request) -> Result<Plan, String> {
    let doc = &request.doc;
    if doc.settings.protection.as_deref().is_some_and(|s| s != "none") {
        return Err("文档限制编辑，请先使用可编辑副本。".into());
    }
    if doc.settings.track_changes {
        return Err("请先在副本中关闭修订记录，再执行翻译。已有修订不会被自动接受。".into());
    }
    if doc.passthrough.keys().any(|p| p.to_ascii_lowercase().ends_with("vbaproject.bin")) {
        return Err("暂不翻译含 VBA 的文档，请先另存不含宏的 DOCX 副本。".into());
    }
    let ranges = match &request.scope {
        Scope::Document => None,
        Scope::Selection(selections) => {
            if selections.is_empty() || selections.len() > 256 {
                return Err("请选择文字，最多支持 256 个选区。".into());
            }
            let mut ranges = Vec::new();
            for s in selections {
                let (a, b) = s.ordered();
                if a.story != b.story {
                    return Err("单个选区不能跨越正文与页眉等不同区域。".into());
                }
                for p in [&a, &b] {
                    if !doc.para_at(p).is_some_and(|para| para.text.is_char_boundary(p.off)) {
                        return Err("文字选区无效。".into());
                    }
                }
                if a != b {
                    ranges.push((a, b));
                }
            }
            if ranges.is_empty() {
                return Err("请先用鼠标选中文字，再点击翻译按钮。".into());
            }
            Some(ranges)
        }
    };
    let mut out = Plan { texts: Vec::new(), targets: Vec::new(), skipped_bilingual: 0, skipped_special: 0 };
    let mut known = HashMap::<String, usize>::new();
    let mut bytes = 0usize;
    let stories =
        std::iter::once(StoryRef::Body).chain(doc.parts.iter().filter(|(_, p)| p.kind != PartKind::Comment).map(|(id, _)| StoryRef::Part(*id)));
    for story in stories {
        if ranges.as_ref().is_some_and(|r| !r.iter().any(|(a, _)| a.story == story)) {
            continue;
        }
        let fields = doc.field_ranges(story);
        let controls = doc.control_ranges(story);
        let paths = doc.para_paths(story);
        if paths.len() > 100_000 {
            return Err("文档段落过多，请分拆文档后翻译。".into());
        }
        for path in paths {
            let para = doc.para(story, &path).ok_or("段落不存在。")?;
            let mut spans = if let Some(ranges) = &ranges {
                ranges
                    .iter()
                    .filter(|(a, b)| a.story == story && a.path <= path && b.path >= path)
                    .filter_map(|(a, b)| {
                        let start = if a.path == path { a.off } else { 0 };
                        let end = if b.path == path { b.off } else { para.len() };
                        (start < end).then_some(start..end)
                    })
                    .collect::<Vec<_>>()
            } else {
                std::iter::once(0..para.len()).collect()
            };
            if spans.is_empty() {
                continue;
            }
            if fields.iter().any(|r| r.start.path <= path && path <= r.end.path)
                || controls.iter().any(|r| r.start.path <= path && path <= r.end.path)
                || para.objects.iter().any(|o| {
                    matches!(
                        o,
                        wordcraft_doc::InlineObject::FieldStart { .. }
                            | wordcraft_doc::InlineObject::FieldEnd
                            | wordcraft_doc::InlineObject::ControlStart { .. }
                            | wordcraft_doc::InlineObject::ControlEnd
                    )
                })
                || para.runs.iter().any(|r| r.props.del.is_some() || r.props.ins.is_some() || r.props.hidden == Some(true))
            {
                out.skipped_special += 1;
                continue;
            }
            spans.sort_by_key(|r| r.start);
            let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
            for span in spans {
                if let Some(last) = merged.last_mut()
                    && last.end >= span.start
                {
                    last.end = last.end.max(span.end);
                } else {
                    merged.push(span);
                }
            }
            for span in merged {
                let piece = para.text.get(span.clone()).ok_or("文字选区不是有效字符边界。")?;
                let mut offset = span.start;
                // Keep line breaks together: original and translation often occupy separate
                // lines in the same paragraph. Only selected text is sent to the reviewer.
                for part in piece.split_inclusive(|c: char| c == wordcraft_doc::para::OBJ || (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))) {
                    let text = part.trim_matches(|c: char| c == wordcraft_doc::para::OBJ || c.is_whitespace() || c.is_control());
                    let end = offset
                        .saturating_add(part.trim_end_matches(|c: char| c == wordcraft_doc::para::OBJ || c.is_whitespace() || c.is_control()).len());
                    offset = offset.saturating_add(part.len());
                    if !eligible(text, request.target) {
                        continue;
                    }
                    if text.len() > 32 * 1024 {
                        return Err("单段待译文字超过 32 KB，请缩小选区。".into());
                    }
                    let index = match known.get(text) {
                        Some(i) => *i,
                        None => {
                            bytes = bytes.saturating_add(text.len());
                            if bytes > 2 * 1024 * 1024 || out.texts.len() >= 2000 {
                                return Err("单次翻译最多 2000 段、2 MB 文字，请分批选择。".into());
                            }
                            let i = out.texts.len();
                            out.texts.push(text.into());
                            known.insert(text.into(), i);
                            i
                        }
                    };
                    if out.targets.len() >= 10_000 {
                        return Err("待译位置超过 1 万个，请分批选择。".into());
                    }
                    let style_at = para.text.get(..end).and_then(|s| s.char_indices().next_back()).map_or(0, |(i, _)| i);
                    out.targets.push(Target { at: Pos::new(story, path.clone(), end), style_at, text: index, paragraph_end: end == para.len() });
                }
            }
        }
    }
    out.targets.sort_by(|a, b| a.at.cmp(&b.at));
    Ok(out)
}

pub struct Applied {
    pub doc: Document,
    pub changed: usize,
    pub already_present: usize,
}
/// `None` is an explicit reviewed skip; missing/invalid entries still fail atomically.
pub fn apply_reviewed(request: &Request, plan: &Plan, translations: &[Option<String>]) -> Result<Applied, String> {
    if translations.len() != plan.texts.len() {
        return Err("网站遗漏了部分译文，原文不变。".into());
    }
    let mut clean = Vec::with_capacity(translations.len());
    let mut size = 0usize;
    for (text, source) in translations.iter().zip(&plan.texts) {
        let Some(text) = text else {
            clean.push(None);
            continue;
        };
        let text = text.trim();
        size = size.saturating_add(text.len());
        if text.is_empty()
            || text == source
            || text.len() > 128 * 1024
            || size > 16 * 1024 * 1024
            || text.chars().any(|c| c == wordcraft_doc::para::OBJ || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
            || !(match request.target {
                Language::English => english(text) && !chinese(text),
                Language::Chinese => chinese(text),
            })
        {
            return Err("网站返回了空白、未翻译或异常文本，未生成副本。".into());
        }
        clean.push(Some(text.replace("\r\n", "\n").replace('\r', "\n")));
    }
    let mut doc = request.doc.clone();
    let mut changed = 0;
    let mut already_present = 0;
    for target in plan.targets.iter().rev() {
        let Some(text) = clean.get(target.text).ok_or("译文索引异常。")? else { continue };
        // Check only the snapshot: an addition for a later selected occurrence must
        // not make an earlier occurrence appear already translated.
        let para = request.doc.para_at(&target.at).ok_or("原段落不存在。")?;
        if para.text.contains(text) {
            already_present += 1;
            continue;
        }
        let props = para.props_of_char(target.style_at).formatting();
        let addition = if target.paragraph_end { format!("\n{text}") } else { format!(" [{text}]") };
        doc.insert_text(&target.at, &addition, &props).map_err(|e| e.to_string())?;
        changed += 1;
    }
    Ok(Applied { doc, changed, already_present })
}

#[cfg(test)]
fn apply(request: &Request, plan: &Plan, translations: &[String]) -> Result<Document, String> {
    apply_reviewed(request, plan, &translations.iter().cloned().map(Some).collect::<Vec<_>>()).map(|r| r.doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wordcraft_doc::{Block, Cell, CharProps, InlineObject, Paragraph, Path, Table, para_block};
    fn para(s: &str) -> Arc<Block> {
        para_block(Paragraph::with_text(s, CharProps::default()))
    }
    fn fixture() -> Document {
        let mut doc = Document::new();
        doc.body = vec![para("阻焊颜色"), para("绿色 Green"), para("Solder mask color"), para("12345"), para("阻焊颜色")];
        doc
    }
    fn selected(doc: Document, a: Pos, b: Pos, target: Language) -> Request {
        Request { doc, scope: Scope::Selection(vec![Selection { anchor: a, focus: b }]), target }
    }
    #[test]
    fn whole_document_deduplicates_and_reviews_bilingual_lines() -> Result<(), String> {
        let request = Request { doc: fixture(), scope: Scope::Document, target: Language::English };
        let p = plan(&request)?;
        assert_eq!(p.texts, vec!["阻焊颜色", "绿色 Green"]);
        assert_eq!(p.changed(), 3);
        let result = apply_reviewed(&request, &p, &[Some("Solder mask color".into()), None])?;
        assert_eq!(result.changed, 2);
        assert_eq!(result.doc.para_at(&Pos::body(0, 0)).ok_or("p")?.text, "阻焊颜色\nSolder mask color");
        let request = Request { doc: result.doc, ..request };
        let repeated = plan(&request)?;
        assert_eq!(repeated.texts[0], "阻焊颜色\nSolder mask color");
        assert_eq!(apply_reviewed(&request, &repeated, &[None, None])?.changed, 0);
        Ok(())
    }
    #[test]
    fn partial_unicode_reverse_selection_preserves_runs_objects_and_other_content() -> Result<(), String> {
        let mut doc = fixture();
        let props = CharProps { bold: Some(true), ..Default::default() };
        let mut paragraph = Paragraph::with_text("前文选择后文", props.clone());
        paragraph.insert_object(0, InlineObject::BookmarkStart { name: "bookmark".into() }, &props).map_err(|e| e.to_string())?;
        doc.body[0] = para_block(paragraph);
        let request = selected(doc, Pos::body(0, 15), Pos::body(0, 9), Language::English);
        let p = plan(&request)?;
        assert_eq!(p.texts, vec!["选择"]);
        let mut result = apply(&request, &p, &["Selection".into()])?;
        assert_eq!(result.para_at(&Pos::body(0, 0)).ok_or("p")?.text, "\u{fffc}前文选择 [Selection]后文");
        assert_eq!(result.para_at(&Pos::body(0, 0)).ok_or("p")?.props_at(16).bold, Some(true));
        result.para_mut(StoryRef::Body, &Path::top(0)).map_err(|e| e.to_string())?.delete(15, 27).map_err(|e| e.to_string())?;
        assert_eq!(result, request.doc, "all original content, formatting, objects and metadata are preserved");
        Ok(())
    }
    #[test]
    fn chinese_selection_leaves_chinese_numbers_and_other_paragraphs_alone() -> Result<(), String> {
        let request = selected(fixture(), Pos::body(1, 0), Pos::body(3, 5), Language::Chinese);
        let p = plan(&request)?;
        assert_eq!(p.texts, vec!["绿色 Green", "Solder mask color"]);
        let result = apply_reviewed(&request, &p, &[None, Some("阻焊颜色".into())])?.doc;
        assert_eq!(result.body.first(), request.doc.body.first());
        assert_eq!(result.body.get(1), request.doc.body.get(1));
        assert_eq!(result.body.get(3), request.doc.body.get(3));
        assert_eq!(result.para_at(&Pos::body(2, 0)).ok_or("p")?.text, "Solder mask color\n阻焊颜色");
        assert_eq!(plan(&Request { doc: result, ..request })?.texts, vec!["绿色 Green", "Solder mask color\n阻焊颜色"]);
        Ok(())
    }
    #[test]
    fn tables_headers_and_textboxes_are_included_but_comments_are_not() -> Result<(), String> {
        let mut doc = Document::new();
        let mut table = Table::new(1, 2, 300.0);
        table.rows[0].cells = vec![Cell::with_text("板材"), Cell::with_text("绿色 Green")];
        doc.body = vec![Arc::new(Block::Table(table))];
        for kind in [PartKind::Header, PartKind::Footer, PartKind::Footnote, PartKind::Endnote, PartKind::TextBox, PartKind::Comment] {
            doc.try_add_part(kind, vec![para("页眉")]).map_err(|e| e.to_string())?;
        }
        let request = Request { doc, scope: Scope::Document, target: Language::English };
        let p = plan(&request)?;
        assert_eq!(p.changed(), 7);
        assert_eq!(p.texts.len(), 3);
        let texts = p
            .texts
            .iter()
            .map(|s| match s.as_str() {
                "板材" => Some("Board material".into()),
                "页眉" => Some("Header".into()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let result = apply_reviewed(&request, &p, &texts)?.doc;
        let bytes = crate::io::save_bytes("translated.docx", &result)?;
        let reopened = crate::io::open_bytes("translated.docx", &bytes)?;
        let at = Pos::new(StoryRef::Body, Path(vec![0, 0, 0, 0]), 0);
        assert!(reopened.para_at(&at).ok_or("table cell")?.text.contains("Board material"));
        Ok(())
    }
    #[test]
    fn malformed_ranges_fields_and_invalid_responses_do_not_modify_the_source() -> Result<(), String> {
        let mut request = selected(fixture(), Pos::body(0, 1), Pos::body(0, 3), Language::English);
        assert!(plan(&request).is_err());
        request.scope = Scope::Selection(vec![Selection::caret(Pos::body(0, 0))]);
        assert!(plan(&request).is_err());
        request.scope = Scope::Document;
        let p = plan(&request)?;
        let before = request.doc.clone();
        for invalid in [vec![], vec!["阻焊颜色".into()], vec!["\u{fffc}English".into()], vec!["\0English".into()]] {
            assert!(apply(&request, &p, &invalid).is_err());
            assert_eq!(request.doc, before);
        }
        let mut doc = fixture();
        doc.settings.track_changes = true;
        assert!(plan(&Request { doc, scope: Scope::Document, target: Language::English }).is_err());
        let mut doc = fixture();
        doc.para_mut(StoryRef::Body, &Path::top(0)).map_err(|e| e.to_string())?.runs[0].props.del = Some(1);
        assert_eq!(plan(&Request { doc, scope: Scope::Document, target: Language::English })?.skipped_special, 1);
        for closed in [false, true] {
            let mut doc = fixture();
            let paragraph = doc.para_mut(StoryRef::Body, &Path::top(0)).map_err(|e| e.to_string())?;
            paragraph
                .insert_object(
                    0,
                    InlineObject::FieldStart { instr: "MERGEFIELD material".into(), locked: false, dirty: false },
                    &CharProps::default(),
                )
                .map_err(|e| e.to_string())?;
            if closed {
                paragraph.insert_object(paragraph.len(), InlineObject::FieldEnd, &CharProps::default()).map_err(|e| e.to_string())?;
            }
            assert_eq!(plan(&Request { doc, scope: Scope::Document, target: Language::English })?.skipped_special, 1);
        }
        Ok(())
    }
    #[test]
    fn selected_mixed_model_does_not_send_unselected_context_and_uses_selected_run() -> Result<(), String> {
        let mut doc = Document::new();
        let mut p = Paragraph::with_text("前文", CharProps { size: Some(24.0), ..Default::default() });
        p.insert_text(p.len(), "孔", &CharProps { size: Some(10.0), ..Default::default() }).map_err(|e| e.to_string())?;
        p.insert_text(p.len(), "后文 English", &CharProps { size: Some(30.0), ..Default::default() }).map_err(|e| e.to_string())?;
        doc.body = vec![para_block(p)];
        let r = selected(doc, Pos::body(0, 6), Pos::body(0, 9), Language::English);
        let p = plan(&r)?;
        assert_eq!(p.texts, vec!["孔"]);
        let result = apply(&r, &p, &["Hole".into()])?;
        let para = result.para_at(&Pos::body(0, 0)).ok_or("p")?;
        assert_eq!(para.text, "前文孔 [Hole]后文 English");
        assert_eq!(para.props_of_char(11).size, Some(10.0));
        assert_eq!(para.props_of_char(17).size, Some(30.0));
        Ok(())
    }
    #[test]
    fn explicit_table_column_ranges_exclude_other_cells_and_keep_reversed_utf8() -> Result<(), String> {
        let mut doc = Document::new();
        let mut t = Table::new(2, 2, 300.0);
        for row in &mut t.rows {
            row.cells = vec![Cell::with_text("PTH孔径0.075mm"), Cell::with_text("不发送")];
        }
        doc.body = vec![Arc::new(Block::Table(t))];
        let selections = (0..2)
            .map(|row| {
                let start = Pos::new(StoryRef::Body, Path(vec![0, row, 0, 0]), 0);
                let mut end = start.clone();
                end.off = "PTH孔径0.075mm".len();
                Selection { anchor: end, focus: start }
            })
            .collect();
        let r = Request { doc, scope: Scope::Selection(selections), target: Language::English };
        let p = plan(&r)?;
        assert_eq!(p.texts, vec!["PTH孔径0.075mm"]);
        assert_eq!(p.changed(), 2);
        let result = apply(&r, &p, &["PTH hole diameter 0.075mm".into()])?;
        assert_eq!(result.para(StoryRef::Body, &Path(vec![0, 0, 1, 0])).ok_or("p")?.text, "不发送");
        Ok(())
    }
    #[test]
    fn selected_source_half_never_appends_exact_existing_translation() -> Result<(), String> {
        let mut doc = Document::new();
        doc.body = vec![para("孔径\nHole diameter")];
        let r = selected(doc, Pos::body(0, 0), Pos::body(0, 6), Language::English);
        let p = plan(&r)?;
        assert_eq!(p.texts, vec!["孔径"]);
        let result = apply_reviewed(&r, &p, &[Some("Hole diameter".into())])?;
        assert_eq!(result.changed, 0);
        assert_eq!(result.already_present, 1);
        assert_eq!(result.doc, r.doc);
        Ok(())
    }
    #[test]
    fn repeated_disjoint_selections_in_one_paragraph_each_receive_their_translation() -> Result<(), String> {
        let mut doc = Document::new();
        doc.body = vec![para("孔/孔")];
        let r = Request {
            doc,
            scope: Scope::Selection(vec![
                Selection { anchor: Pos::body(0, 0), focus: Pos::body(0, 3) },
                Selection { anchor: Pos::body(0, 4), focus: Pos::body(0, 7) },
            ]),
            target: Language::English,
        };
        let p = plan(&r)?;
        assert_eq!(p.texts, vec!["孔"]);
        let result = apply_reviewed(&r, &p, &[Some("Hole".into())])?;
        assert_eq!(result.changed, 2);
        assert_eq!(result.doc.para_at(&Pos::body(0, 0)).ok_or("p")?.text, "孔 [Hole]/孔\nHole");
        Ok(())
    }
}

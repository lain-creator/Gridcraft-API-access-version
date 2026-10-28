//! Local, user-imported terminology. Never contacts the glossary website or ships user data.
use std::{collections::BTreeSet, io::Read, path::Path};

use gridcraft_ui_egui::ai::GlossaryInfo;
use serde::{Deserialize, Serialize};

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TERMS: usize = 5000;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Term {
    pub en: String,
    pub zh: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}
#[derive(Default)]
pub struct Glossary {
    terms: Vec<Term>,
    source: String,
}
#[derive(Deserialize, Serialize)]
struct Document {
    #[serde(default)]
    source: String,
    items: Vec<Term>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Input {
    List(Vec<Term>),
    Object(Document),
}

// This contradictory source pair describes foil, not copper-clad material. Keep the
// original record for audit, but never instruct a model to use a known incorrect pair.
fn quarantined(term: &Term) -> bool {
    term.en.eq_ignore_ascii_case("Copper clad") && term.zh == "铜箔"
}
fn valid_field(s: &str, limit: usize) -> bool {
    s.chars().count() <= limit && !s.chars().any(char::is_control)
}
impl Glossary {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_BYTES {
            return Err("术语库不能超过 4 MB。".into());
        }
        let input: Input = serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .map_err(|_| "术语库须为 JSON 数组或含 items 数组的对象，每条含 en、zh、category、aliases。")?;
        let (mut terms, source) = match input {
            Input::List(terms) => (terms, String::new()),
            Input::Object(doc) => (doc.items, doc.source),
        };
        if terms.is_empty() || terms.len() > MAX_TERMS || !valid_field(&source, 200) {
            return Err("术语库须包含 1–5000 条有效术语，来源说明最长 200 字。".into());
        }
        let mut seen = BTreeSet::new();
        for term in &mut terms {
            term.en = term.en.trim().to_string();
            term.zh = term.zh.trim().to_string();
            term.category = term.category.trim().to_string();
            if term.en.is_empty()
                || term.zh.is_empty()
                || !valid_field(&term.en, 200)
                || !valid_field(&term.zh, 200)
                || !valid_field(&term.category, 100)
                || term.aliases.len() > 24
                || term.aliases.iter().any(|a| a.trim().is_empty() || !valid_field(a, 200))
                || !seen.insert((term.en.to_ascii_lowercase(), term.zh.clone()))
            {
                return Err("术语库含空白、重复词条、控制字符或过长字段，原词库未更改。".into());
            }
        }
        Ok(Self { terms, source })
    }
    pub fn info(&self) -> GlossaryInfo {
        let mut translations = std::collections::BTreeMap::<&str, BTreeSet<String>>::new();
        for term in self.terms.iter().filter(|t| !quarantined(t)) {
            translations.entry(&term.zh).or_default().insert(term.en.to_ascii_lowercase());
        }
        GlossaryInfo {
            entries: self.terms.len(),
            quarantined: self.terms.iter().filter(|t| quarantined(t)).count(),
            ambiguous: translations.values().filter(|v| v.len() > 1).count(),
            source: self.source.clone(),
        }
    }
    /// Longest phrase wins; exact same spans keep alternative mappings for contextual
    /// disambiguation. English aliases require word boundaries (Core is not sCORE).
    pub fn relevant(&self, text: &str) -> Result<Vec<&Term>, String> {
        if text.len() > 128 * 1024 {
            return Err("单条术语匹配内容超过 128 KB，请缩小选区。".into());
        }
        let lower = text.to_ascii_lowercase();
        let mut candidates = Vec::new();
        for (index, term) in self.terms.iter().enumerate().filter(|(_, t)| !quarantined(t)) {
            for form in std::iter::once(&term.zh).chain(std::iter::once(&term.en)).chain(&term.aliases) {
                let form = form.trim().to_ascii_lowercase();
                for (start, matched) in lower.match_indices(&form) {
                    let end = start.saturating_add(matched.len());
                    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
                    if (form.as_bytes().first().is_some_and(|b| word(*b))
                        && start.checked_sub(1).and_then(|i| lower.as_bytes().get(i)).is_some_and(|b| word(*b)))
                        || (form.as_bytes().last().is_some_and(|b| word(*b)) && lower.as_bytes().get(end).is_some_and(|b| word(*b)))
                    {
                        continue;
                    }
                    if candidates.len() >= 20_000 {
                        return Err("术语命中过多，请缩小选区；未截断词库。".into());
                    }
                    candidates.push((start, end, index));
                }
            }
        }
        candidates.sort_by_key(|(s, e, i)| (std::cmp::Reverse(e.saturating_sub(*s)), *s, *i));
        let mut spans = Vec::new();
        let mut indices = BTreeSet::new();
        for (start, end, index) in candidates {
            if spans.iter().any(|(s, e)| start < *e && end > *s && (start != *s || end != *e)) {
                continue;
            }
            spans.push((start, end));
            indices.insert(index);
            if indices.len() > 128 {
                return Err("单条内容命中超过 128 个术语，请分批翻译；未静默截断。".into());
            }
        }
        Ok(indices.into_iter().filter_map(|i| self.terms.get(i)).collect())
    }
}

fn read(path: &Path) -> Result<Glossary, String> {
    let file = std::fs::File::open(path).map_err(|_| "无法读取术语库文件。")?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| "读取术语库失败。")?;
    Glossary::parse(&bytes)
}
pub fn load() -> Result<Glossary, String> {
    if !super::prefs_enabled() {
        return Ok(Glossary::default());
    }
    let path = super::config_dir().ok_or("无法定位术语库目录。")?.join("pcb-glossary.json");
    if !path.try_exists().map_err(|_| "无法检查术语库文件。")? {
        return Ok(Glossary::default());
    }
    read(&path)
}
/// None reads status; Some imports a validated file atomically. No dialogs or network.
pub fn manage(path: Option<&str>) -> Result<GlossaryInfo, String> {
    let Some(path) = path else { return load().map(|g| g.info()) };
    if !super::prefs_enabled() {
        return Err("测试模式不读写个人术语库。".into());
    }
    let mut glossary = read(Path::new(path))?;
    glossary.source = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("本地 JSON 词库").chars().take(200).collect();
    let dir = super::config_dir().ok_or("无法定位术语库目录。")?;
    std::fs::create_dir_all(&dir).map_err(|_| "无法创建术语库目录。")?;
    save(&dir.join("pcb-glossary.json"), &glossary)?;
    Ok(glossary.info())
}
fn save(path: &Path, glossary: &Glossary) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().ok_or("术语库目录无效。")?;
    let bytes =
        serde_json::to_vec_pretty(&Document { source: glossary.source.clone(), items: glossary.terms.clone() }).map_err(|_| "无法编码术语库。")?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|_| "无法创建术语库临时文件。")?;
    temp.write_all(&bytes).map_err(|_| "无法保存术语库。")?;
    temp.persist(path).map_err(|_| "无法替换术语库，原文件保持不变。")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    pub fn fixture() -> Result<Glossary, String> {
        Glossary::parse(
            json!([
                {"en":"Lamination","zh":"压合"},
                {"en":"Lay up setup","zh":"压合结构"},
                {"en":"Core","zh":"芯板","aliases":["芯板（芯料）"]},
                {"en":"Stackup","zh":"叠层结构","aliases":["Layer stack"]},
                {"en":"Pad","zh":"焊盘"}, {"en":"Land","zh":"焊盘"},
                {"en":"Copper foil","zh":"铜箔"}, {"en":"Copper clad","zh":"铜箔"}
            ])
            .to_string()
            .as_bytes(),
        )
    }
    #[test]
    fn longest_match_alias_boundaries_conflicts_and_quarantine() -> TestResult {
        let g = fixture()?;
        assert_eq!(g.relevant("压合结构")?.iter().map(|t| t.en.as_str()).collect::<Vec<_>>(), ["Lay up setup"]);
        assert_eq!(g.relevant("Layer stack, CORE")?.len(), 2);
        assert!(g.relevant("score delamination coreless")?.is_empty());
        assert_eq!(g.relevant("焊盘")?.len(), 2);
        assert_eq!(g.relevant("铜箔")?.iter().map(|t| t.en.as_str()).collect::<Vec<_>>(), ["Copper foil"]);
        assert_eq!((g.info().entries, g.info().quarantined, g.info().ambiguous), (8, 1, 1));
        Ok(())
    }
    #[test]
    fn reject_invalid_limits_without_replacing_existing_data() -> TestResult {
        for bad in ["[]", "null", "{\"items\":[{}]}", "[{\"en\":\"x\",\"zh\":\"\"}]", "[{\"en\":\"x\",\"zh\":\"中\",\"aliases\":[\"\"]}]"] {
            assert!(Glossary::parse(bad.as_bytes()).is_err());
        }
        assert!(Glossary::parse(&vec![b' '; MAX_BYTES as usize + 1]).is_err());
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("glossary.json");
        save(&path, &fixture()?)?;
        assert_eq!(read(&path)?.info().entries, 8);
        assert!(read(&dir.path().join("missing")).is_err());
        assert_eq!(read(&path)?.info().entries, 8);
        Ok(())
    }
}

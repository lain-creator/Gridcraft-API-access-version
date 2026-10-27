//! Reading one catalog: `context<TAB>English source text<TAB>translation` rows, checked against
//! their source text. Format: `docs/catalog-format.md`.

use std::collections::HashSet;

/// A catalog row that passed every check, with its columns unescaped.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Row {
    /// 1-based line number in the file.
    pub line: usize,
    pub context: String,
    pub source: String,
    pub translation: String,
}

/// The contexts the apps give a meaning to. Any other context starting with `@` is an error;
/// contexts without `@` are free labels.
pub const RESERVED: &[&str] = &["@plural", "@id", "@msg"];

/// Read a catalog whose locale has `forms` plural forms. Returns the rows that pass, and one
/// problem (`line N: …`) per row that doesn't; a bad row is left out.
pub fn read_catalog(text: &str, forms: usize) -> (Vec<Row>, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    let mut keys = HashSet::new();
    for (i, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let row = parse_row(i + 1, line, forms).and_then(|row| {
            if keys.insert((row.context.clone(), row.source.clone())) {
                Ok(row)
            } else {
                Err("defined twice (same context and source text)".into())
            }
        });
        match row {
            Ok(row) => rows.push(row),
            Err(e) => problems.push(format!("line {}: {e}", i + 1)),
        }
    }
    (rows, problems)
}

fn parse_row(line: usize, text: &str, forms: usize) -> Result<Row, String> {
    let columns: Vec<&str> = text.split('\t').collect();
    let [context, source, translation] = columns.as_slice() else {
        return Err(format!("{} columns; expected 3", columns.len()));
    };
    if let Some(c) = text
        .chars()
        .find(|&c| c != '\t' && (c.is_control() || c == '\u{feff}'))
    {
        return Err(format!(
            "invisible character {c:?} in the row; write \\n or \\t, or delete it"
        ));
    }
    if source.is_empty() {
        return Err("empty source text".into());
    }
    let row = Row {
        line,
        context: unescape(context)?,
        source: unescape(source)?,
        translation: unescape(translation)?,
    };
    if row.translation.is_empty() {
        return Err("empty translation; leave the row out instead".into());
    }
    match row.context.as_str() {
        // The source is a command id, not a template.
        "@id" => {}
        "@plural" => check_plural(&row.source, &row.translation, forms)?,
        c if c.starts_with('@') && !RESERVED.contains(&c) => {
            return Err(format!(
                "unknown reserved context {c:?}; use one of {RESERVED:?}"
            ));
        }
        _ => {
            if placeholders(&row.source) != placeholders(&row.translation) {
                return Err("placeholders differ from the source text".into());
            }
            if row.source.ends_with('…') != row.translation.ends_with('…') {
                return Err("trailing … differs from the source text".into());
            }
            if row.context == "@msg" {
                check_template(&row.source)?;
            }
        }
    }
    Ok(row)
}

/// An `@msg` source text with `{` is a template whose placeholders match any text. The apps skip
/// a template that can never match, so the string would silently stay English. Rules:
/// - `{` with no closing `}` (craft-launcher's `Template::new`, vectorcraft's `Template::parse`);
/// - two placeholders in a row, as where one ends and the next begins is unknowable (same two);
/// - no literal text, `{x}` alone, which would match every message (vectorcraft's
///   `Template::parse`; craft-launcher accepts it, and the stricter rule holds for both).
fn check_template(source: &str) -> Result<(), String> {
    let mut literal = 0;
    let mut after_placeholder = false;
    let mut rest = source;
    while let Some((before, after_open)) = rest.split_once('{') {
        let Some((_, after)) = after_open.split_once('}') else {
            return Err("@msg template has { with no closing }; it can never match".into());
        };
        if before.is_empty() && after_placeholder {
            return Err("@msg template has two placeholders in a row; it can never match".into());
        }
        literal += before.len();
        after_placeholder = true;
        rest = after;
    }
    if after_placeholder && literal + rest.len() == 0 {
        return Err("@msg template has no literal text; it would match any message".into());
    }
    Ok(())
}

fn check_plural(source: &str, translation: &str, forms: usize) -> Result<(), String> {
    let english: Vec<&str> = source.split('|').collect();
    let [one, other] = english.as_slice() else {
        return Err("plural source text must be one|other".into());
    };
    if one.is_empty() || other.is_empty() || placeholders(one) != placeholders(other) {
        return Err("plural source forms must be nonempty with the same placeholders".into());
    }
    let translated: Vec<&str> = translation.split('|').collect();
    if translated.len() != forms || translated.iter().any(|f| f.is_empty()) {
        return Err(format!("expected {forms} nonempty plural forms"));
    }
    if translated
        .iter()
        .any(|f| placeholders(f) != placeholders(other))
    {
        return Err("plural form placeholders differ from the source text".into());
    }
    Ok(())
}

fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => return Err(format!("unknown escape \\{other}; use \\n, \\t or \\\\")),
            None => return Err("trailing backslash; use \\\\ for a backslash".into()),
        }
    }
    Ok(out)
}

/// `{name}` placeholders, sorted: translations may reorder them.
fn placeholders(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once('{') {
        let Some((name, after)) = after.split_once('}') else {
            break;
        };
        names.push(name);
        rest = after;
    }
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_rows_pass() {
        let ok = "# c\n\n\tSave\tEnregistrer\nmenu\tOpen\tOuvrir\n@id\tfile.save\tEnregistrer\n@msg\tno file `{name}`\taucun fichier `{name}`\n@plural\t{n} page|{n} pages\t{n} page|{n} pages\n";
        let (rows, problems) = read_catalog(ok, 2);
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].line, 3);
    }

    #[test]
    fn context_distinguishes_keys() {
        assert!(
            read_catalog("\tOpen\tOuvrir\nmenu\tOpen\tOuvert\n", 2)
                .1
                .is_empty()
        );
    }

    #[test]
    fn each_bad_row_is_one_problem() {
        for (text, line) in [
            ("\tSave\n", 1),
            ("\tSave\tA\tB\n", 1),
            ("\tSave\tA\n\tSave\tB\n", 2),
            ("\tSave\t\n", 1),
            ("\t{n} files\t{count} fichiers\n", 1),
            ("\tOpen…\tOuvrir\n", 1),
            ("\tA\\qB\tA\n", 1),
            ("\tA\\\tA\n", 1),
            ("@nope\tA\tB\n", 1),
            ("@plural\tpage\tpage|pages\n", 1),
            ("@plural\t{n} page|pages\tx|y\n", 1),
            ("@plural\t{n} a|{n} b\t{n} x\n", 1),
            ("@plural\t{n} a|{n} b\t{n} x|y\n", 1),
            ("\tSave\tEnregistrer\r\r\n", 1),
            ("\tSave\tEn\rreg\n", 1),
            ("\tA\tB\n\u{feff}\tC\tD\n", 2),
            ("\t\tVide\n", 1),
            ("@msg\t{x}\t{x}\n", 1),
            ("@msg\tOpen {a}{b}\tOuvrir {a}{b}\n", 1),
            ("@msg\tOpen {a\tOuvrir {a\n", 1),
        ] {
            let problems = read_catalog(text, 2).1;
            assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
            assert!(
                problems[0].starts_with(&format!("line {line}:")),
                "{text:?}: {problems:?}"
            );
        }
    }

    #[test]
    fn placeholders_may_reorder() {
        assert!(read_catalog("\t{a} to {b}\t{b} vers {a}\n", 2).1.is_empty());
    }

    #[test]
    fn escapes_unescape() {
        let (rows, problems) = read_catalog("\tA\\nB\tC\\tD\\\\\n", 2);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(rows[0].source, "A\nB");
        assert_eq!(rows[0].translation, "C\tD\\");
    }

    #[test]
    fn one_form_locale() {
        assert!(
            read_catalog("@plural\t{n} a|{n} b\t{n} x\n", 1)
                .1
                .is_empty()
        );
    }

    #[test]
    fn crlf_and_bom_are_ignored() {
        let (rows, problems) =
            read_catalog("\u{feff}\tSave\tEnregistrer\r\n\tUndo\tAnnuler\r\n", 2);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(rows[0].context, "");
        assert_eq!(rows[1].translation, "Annuler");
    }
}

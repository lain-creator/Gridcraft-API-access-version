//! Checking every catalog under a checkout's `locales/`: `locales/<locale>/common.tsv` (shared
//! strings) and `locales/<locale>/<app>.tsv` (one app's strings and overrides).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::catalog::{Row, read_catalog};
use crate::locale::{check_locale_name, plural_forms};

/// The shared catalog's file name.
pub const COMMON: &str = "common.tsv";

/// (context, source text) to translation: one catalog's rows.
type Rows = HashMap<(String, String), String>;

/// One locale's catalogs: the shared rows and each app's rows by app name (file name without
/// `.tsv`).
#[derive(Default)]
struct Locale {
    common: Rows,
    apps: BTreeMap<String, Rows>,
}

/// Every problem under `root/locales`, each prefixed by its path relative to `root`. No
/// `locales/` directory means no locales, and no problems.
pub fn check_tree(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let mut catalogs: BTreeMap<String, Locale> = BTreeMap::new();
    let locales = match entries(&root.join("locales")) {
        Ok(locales) => locales,
        Err(e) => return vec![format!("locales: {e}")],
    };
    for locale in locales {
        let dir = root.join("locales").join(&locale);
        if !dir.is_dir() {
            problems.push(format!("locales/{locale}: not a locale directory"));
            continue;
        }
        if let Err(e) = check_locale_name(&locale) {
            problems.push(format!("locales/{locale}: {e}"));
            continue;
        }
        let forms = plural_forms(&locale).unwrap_or_else(|e| {
            problems.push(format!("locales/{locale}: {e}"));
            1
        });
        let files = match entries(&dir) {
            Ok(files) => files,
            Err(e) => {
                problems.push(format!("locales/{locale}: {e}"));
                continue;
            }
        };
        let shared: HashSet<Row> = read(&dir.join(COMMON), forms)
            .map(|(rows, _)| rows.into_iter().map(key).collect())
            .unwrap_or_default();
        let loaded = catalogs.entry(locale.clone()).or_default();
        for file in files {
            let path = format!("locales/{locale}/{file}");
            if !is_catalog_name(&file) {
                problems.push(format!(
                    "{path}: not a catalog; name it {COMMON} or <app>.tsv (lowercase, digits, -)"
                ));
                continue;
            }
            let (rows, text_problems) = match read(&dir.join(&file), forms) {
                Ok(read) => read,
                Err(e) => {
                    problems.push(format!("{path}: {e}"));
                    continue;
                }
            };
            problems.extend(text_problems.into_iter().map(|p| format!("{path}: {p}")));
            if file != COMMON {
                for row in rows.iter().filter(|r| shared.contains(&key((*r).clone()))) {
                    problems.push(format!(
                        "{path}: line {}: same translation as {COMMON}; remove the override",
                        row.line
                    ));
                }
            }
            let table: Rows = rows
                .into_iter()
                .map(|r| ((r.context, r.source), r.translation))
                .collect();
            if file == COMMON {
                loaded.common = table;
            } else if let Some(app) = file.strip_suffix(".tsv") {
                loaded.apps.insert(app.to_owned(), table);
            }
        }
    }
    for (locale, loaded) in &catalogs {
        if !has_language_name(loaded) {
            problems.push(format!(
                "locales/{locale}: no `@id<TAB>language.name<TAB><name>` row in {COMMON} or an app catalog; the language picker would show the code"
            ));
        }
    }
    problems.extend(redundant_regional_rows(&catalogs));
    problems
}

/// The row the apps' language pickers read: `@id language.name`.
fn has_language_name(locale: &Locale) -> bool {
    let key = ("@id".to_owned(), "language.name".to_owned());
    locale.common.contains_key(&key) || locale.apps.values().any(|rows| rows.contains_key(&key))
}

/// The locale `locale` falls back to, as in craft-libs' `craft-i18n` (`I18n::new`): only a
/// trailing region subtag (two letters or three digits) is dropped, and only when that locale is
/// in the tree too. `pt-br` has no fallback without a `pt/`; `sr-latn`, `zh-hans` and `zh-hant`
/// never have one, since a script is not a region (`sr-latn` to `sr` would show Cyrillic).
fn base_locale<'a>(locale: &str, catalogs: &'a BTreeMap<String, Locale>) -> Option<&'a Locale> {
    let (parent, region) = locale.rsplit_once('-')?;
    let is_region = (region.len() == 2 && region.bytes().all(|b| b.is_ascii_lowercase()))
        || (region.len() == 3 && region.bytes().all(|b| b.is_ascii_digit()));
    if is_region {
        catalogs.get(parent)
    } else {
        None
    }
}

/// The keys an app tries for a row, in order: its own, then (for a label context) the plain row
/// of the same source text, as `tc` does in craft-libs. `@id` and `@plural` rows have no plain
/// fallback.
fn lookup_keys(context: &str, source: &str) -> Vec<(String, String)> {
    let own = (context.to_owned(), source.to_owned());
    if context.is_empty() || context == "@id" || context == "@plural" {
        vec![own]
    } else {
        vec![own, (String::new(), source.to_owned())]
    }
}

/// A row of the regional locale hidden from a lookup: its file (`None` for `common.tsv`, else the
/// app) and key.
type Hidden<'a> = (Option<&'a str>, &'a (String, String));

/// What `app` shows for `keys` in `locale`, then in `base`, as craft-libs' `find` walks the
/// chain: per locale, the first key found, the app's rows over the shared ones. `hidden` is a row
/// of `locale` to leave out.
fn resolve<'a>(
    app: Option<&str>,
    keys: &[(String, String)],
    locale: &'a Locale,
    base: &'a Locale,
    hidden: Option<Hidden<'_>>,
) -> Option<&'a str> {
    let in_locale = |l: &'a Locale, hidden: Option<Hidden<'_>>| {
        keys.iter().find_map(|key| {
            let is_hidden = |file: Option<&str>| hidden == Some((file, key));
            let from_app = app
                .filter(|_| !is_hidden(app))
                .and_then(|a| l.apps.get(a))
                .and_then(|rows| rows.get(key));
            from_app
                .or_else(|| l.common.get(key).filter(|_| !is_hidden(None)))
                .map(String::as_str)
        })
    };
    in_locale(locale, hidden).or_else(|| in_locale(base, None))
}

/// Every regional row that removing would not change: for each app that shows it, the lookup
/// gives the same text through the base language, since the runtime falls back region to base
/// (see [`base_locale`]). Reported as `locales/<region>/<file>: "<context>" "<source>": …`.
fn redundant_regional_rows(catalogs: &BTreeMap<String, Locale>) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, locale) in catalogs {
        let Some(base) = base_locale(name, catalogs) else {
            continue;
        };
        let base_name = name.rsplit_once('-').map_or("", |(parent, _)| parent);
        let apps: Vec<&str> = locale
            .apps
            .keys()
            .chain(base.apps.keys())
            .map(String::as_str)
            .collect();
        let files = std::iter::once((None, &locale.common)).chain(
            locale
                .apps
                .iter()
                .map(|(app, rows)| (Some(app.as_str()), rows)),
        );
        for (file, rows) in files {
            // A shared row shows in every app (and in an app with no catalog of its own); an app
            // row only in its app.
            let shown_in: Vec<Option<&str>> = match file {
                Some(app) => vec![Some(app)],
                None => std::iter::once(None)
                    .chain(apps.iter().copied().map(Some))
                    .collect(),
            };
            for key in rows.keys() {
                let keys = lookup_keys(&key.0, &key.1);
                let redundant = shown_in.iter().all(|&app| {
                    let kept = resolve(app, &keys, locale, base, None);
                    kept.is_some() && kept == resolve(app, &keys, locale, base, Some((file, key)))
                });
                if redundant {
                    let file_name = file.map_or_else(|| COMMON.to_owned(), |a| format!("{a}.tsv"));
                    problems.push(format!(
                        "locales/{name}/{file_name}: {:?} {:?}: same translation as {base_name}; remove the row",
                        key.0, key.1
                    ));
                }
            }
        }
    }
    problems.sort();
    problems
}

/// One line per catalog, `<locale>/<file>: <rows>/<max>`, where `<max>` is the most rows any
/// locale's catalog of that name has. A report only: unreadable files count as empty.
pub fn coverage(root: &Path) -> Vec<String> {
    let mut counts = Vec::new();
    for locale in entries(&root.join("locales")).unwrap_or_default() {
        let dir = root.join("locales").join(&locale);
        let forms = plural_forms(&locale).unwrap_or(1);
        for file in entries(&dir).unwrap_or_default() {
            if is_catalog_name(&file) {
                let rows = read(&dir.join(&file), forms).map_or(0, |(rows, _)| rows.len());
                counts.push((locale.clone(), file, rows));
            }
        }
    }
    let mut max: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, file, rows) in &counts {
        let m = max.entry(file).or_default();
        *m = (*m).max(*rows);
    }
    counts
        .iter()
        .map(|(locale, file, rows)| {
            let m = max.get(file.as_str()).copied().unwrap_or(0);
            format!("{locale}/{file}: {rows}/{m}")
        })
        .collect()
}

/// A catalog's rows and row problems, plus a problem if the clean-room rule is missing.
fn read(path: &Path, forms: usize) -> Result<(Vec<Row>, Vec<String>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (rows, mut problems) = read_catalog(&text, forms);
    if !text.lines().any(|l| l.starts_with("# Clean room:")) {
        problems.insert(
            0,
            "no \"# Clean room:\" line; copy the header from docs/catalog-format.md".into(),
        );
    }
    Ok((rows, problems))
}

/// A row without its line number, to compare rows across files.
fn key(row: Row) -> Row {
    Row { line: 0, ..row }
}

/// `common.tsv`, or `<app>.tsv` with `<app>` in `[a-z0-9]+(-[a-z0-9]+)*`.
fn is_catalog_name(file: &str) -> bool {
    file.strip_suffix(".tsv").is_some_and(|app| {
        app.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
    })
}

/// The names in `dir`, sorted, without dotfiles. A missing `dir` has none.
fn entries(dir: &Path) -> std::io::Result<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

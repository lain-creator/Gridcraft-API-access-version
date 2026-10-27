//! The checker run over this repository's `locales/`, and over small fixture trees.

use std::path::{Path, PathBuf};

use craft_i18n_tests::tree::{check_tree, coverage};

const H: &str = "# Clean room: written from the plain meaning.\n";
/// The row every locale needs for its language picker.
const NAME: &str = "@id\tlanguage.name\tNom\n";
/// A regional locale names itself differently from its base.
const NAME_REGION: &str = "@id\tlanguage.name\tNom (région)\n";

#[test]
fn repository_catalogs_pass() {
    assert_eq!(
        check_tree(&craft_i18n_tests::repo_root()),
        Vec::<String>::new()
    );
}

/// Prints the coverage report (`cargo test coverage_report -- --nocapture`); never fails.
#[test]
fn coverage_report() {
    for line in coverage(&craft_i18n_tests::repo_root()) {
        println!("{line}");
    }
}

/// A fresh tree under the test target directory holding `files` (path relative to the root).
fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn assert_problems(name: &str, files: &[(&str, &str)], expected: &[&str]) {
    let problems = check_tree(&fixture(name, files));
    assert_eq!(problems.len(), expected.len(), "{problems:?}");
    for (problem, prefix) in problems.iter().zip(expected) {
        assert!(problem.starts_with(prefix), "{problem:?} vs {prefix:?}");
    }
}

#[test]
fn clean_tree_passes() {
    let common = format!("{H}{NAME}\tSave\tEnregistrer\n");
    let app = format!("{H}\tSave\tSauvegarder\n");
    assert_problems(
        "clean",
        &[
            ("locales/fr/common.tsv", &common),
            ("locales/fr/wordcraft.tsv", &app),
            ("locales/fr/.DS_Store", ""),
        ],
        &[],
    );
}

#[test]
fn bad_row_names_file_and_line() {
    let common = format!("{H}{NAME}\tSave\n");
    assert_problems(
        "bad-row",
        &[("locales/fr/common.tsv", &common)],
        &["locales/fr/common.tsv: line 3:"],
    );
}

#[test]
fn bad_locale_name() {
    assert_problems(
        "bad-locale",
        &[("locales/pt_BR/common.tsv", H)],
        &["locales/pt_BR:"],
    );
}

#[test]
fn stray_file() {
    let named = format!("{H}{NAME}");
    assert_problems(
        "stray",
        &[
            ("locales/fr/common.tsv", &named),
            ("locales/fr/common.ftl", ""),
        ],
        &["locales/fr/common.ftl:"],
    );
}

#[test]
fn bad_app_file_name() {
    let named = format!("{H}{NAME}");
    assert_problems(
        "bad-app",
        &[
            ("locales/fr/common.tsv", &named),
            ("locales/fr/Word_Craft.tsv", H),
        ],
        &["locales/fr/Word_Craft.tsv:"],
    );
}

#[test]
fn missing_clean_room_rule() {
    assert_problems(
        "no-clean-room",
        &[(
            "locales/fr/common.tsv",
            &format!("{NAME}\tSave\tEnregistrer\n"),
        )],
        &["locales/fr/common.tsv:"],
    );
}

#[test]
fn redundant_override() {
    let same = format!("{H}\tSave\tEnregistrer\n");
    let common = format!("{same}{NAME}");
    assert_problems(
        "redundant",
        &[
            ("locales/fr/common.tsv", &common),
            ("locales/fr/wordcraft.tsv", &same),
        ],
        &["locales/fr/wordcraft.tsv: line 2:"],
    );
}

#[test]
fn plural_count_by_locale() {
    let common = format!("{H}{NAME}@plural\t{{n}} a|{{n}} b\t{{n}} x|{{n}} y\n");
    assert_problems(
        "plural-ja",
        &[("locales/ja/common.tsv", &common)],
        &["locales/ja/common.tsv: line 3:"],
    );
}

#[test]
fn msg_templates_one_good_two_bad() {
    // Good: literal text around a placeholder. Bad: no literal text; two placeholders in a row.
    let common = format!(
        "{H}{NAME}@msg\tCouldn't open {{name}}\tImpossible d'ouvrir {{name}}\n@msg\t{{x}}\t{{x}}\n@msg\tOpen {{a}}{{b}}\tOuvrir {{a}}{{b}}\n"
    );
    assert_problems(
        "msg-templates",
        &[("locales/fr/common.tsv", &common)],
        &[
            "locales/fr/common.tsv: line 4:",
            "locales/fr/common.tsv: line 5:",
        ],
    );
}

#[test]
fn redundant_regional_row() {
    let es = format!("{H}{NAME}\tSave\tGuardar\n\tOpen\tAbrir\n");
    let mx = format!("{H}{NAME_REGION}\tSave\tGuardar\n\tOpen\tAbrir ahora\n");
    assert_problems(
        "regional",
        &[
            ("locales/es/common.tsv", &es),
            ("locales/es-mx/common.tsv", &mx),
        ],
        &["locales/es-mx/common.tsv: \"\" \"Save\": same translation as es"],
    );
}

#[test]
fn regional_app_row_compares_with_base_app_then_shared() {
    let common = format!("{H}{NAME}\tSave\tGuardar\n");
    let es_app = format!("{H}\tSave\tSalvar\n");
    // In wordcraft the base's app row wins: this row, equal to the shared one, is not redundant.
    let mx_differs = format!("{H}\tSave\tGuardar\n");
    // In pdfcraft the base has no app row, so the shared one decides: redundant.
    let mx_same = format!("{H}\tSave\tGuardar\n");
    let mx_common = format!("{H}{NAME_REGION}");
    assert_problems(
        "regional-app",
        &[
            ("locales/es/common.tsv", &common),
            ("locales/es/wordcraft.tsv", &es_app),
            ("locales/es-mx/common.tsv", &mx_common),
            ("locales/es-mx/wordcraft.tsv", &mx_differs),
            ("locales/es-mx/pdfcraft.tsv", &mx_same),
        ],
        &["locales/es-mx/pdfcraft.tsv: \"\" \"Save\": same translation as es"],
    );
}

#[test]
fn script_and_baseless_locales_have_no_regional_check() {
    // `sr-latn` is a script, `pt-br` has no `pt/` here: rows equal to another locale's stay.
    let row = format!("{H}{NAME}\tSave\tSacuvaj\n");
    assert_problems(
        "script",
        &[
            ("locales/sr/common.tsv", &row),
            ("locales/sr-latn/common.tsv", &row),
            ("locales/pt-br/common.tsv", &row),
        ],
        &[],
    );
}

#[test]
fn missing_language_name() {
    let named = format!("{H}{NAME}\tSave\tEnregistrer\n");
    let unnamed = format!("{H}\tSave\tSpeichern\n");
    let app_named = format!("{H}{NAME}");
    assert_problems(
        "language-name",
        &[
            ("locales/fr/common.tsv", &named),
            ("locales/de/common.tsv", &unnamed),
            ("locales/it/wordcraft.tsv", &app_named),
        ],
        &["locales/de:"],
    );
}

#[test]
fn no_locales_dir() {
    assert_problems("empty", &[], &[]);
}

#[test]
fn coverage_counts_rows_against_largest_locale() {
    let fr = format!("{H}\tSave\tEnregistrer\n\tUndo\tAnnuler\n");
    let de = format!("{H}\tSave\tSpeichern\n");
    let report = coverage(&fixture(
        "coverage",
        &[
            ("locales/fr/common.tsv", &fr),
            ("locales/de/common.tsv", &de),
        ],
    ));
    assert!(
        report.contains(&"de/common.tsv: 1/2".to_owned()),
        "{report:?}"
    );
    assert!(
        report.contains(&"fr/common.tsv: 2/2".to_owned()),
        "{report:?}"
    );
}

//! Locale directory names and how many plural forms a locale's `@plural` rows list.

use intl_pluralrules::{PluralRuleType, PluralRules};
use unic_langid::LanguageIdentifier;

/// Ok when `name` is a BCP 47 tag written in lowercase (`pt-br`, `zh-hans`).
pub fn check_locale_name(name: &str) -> Result<(), String> {
    let id: LanguageIdentifier = name
        .parse()
        .map_err(|e| format!("{name:?} is not a BCP 47 locale: {e}"))?;
    let lower = id.to_string().to_lowercase();
    if lower == name {
        Ok(())
    } else {
        Err(format!("{name:?}: write it as {lower:?}"))
    }
}

/// How many plural forms `locale`'s `@plural` rows list: the CLDR cardinal categories whole
/// numbers 0..=1000 reach. Categories only fractions or huge numbers reach (French `many`) don't
/// count, as in the apps. CLDR's `pt` is Brazilian, so Portuguese other than `pt-br` takes
/// `pt-PT`'s rules, as craft-libs' `craft-i18n` does.
pub fn plural_forms(locale: &str) -> Result<usize, String> {
    let rules_for = if locale == "pt" || (locale.starts_with("pt-") && locale != "pt-br") {
        "pt-pt"
    } else {
        locale
    };
    let id: LanguageIdentifier = rules_for
        .parse()
        .map_err(|e| format!("{locale:?} is not a BCP 47 locale: {e}"))?;
    let language = LanguageIdentifier::from_parts(id.language, None, None, &[]);
    let rules = PluralRules::create(id, PluralRuleType::CARDINAL)
        .or_else(|_| PluralRules::create(language, PluralRuleType::CARDINAL))
        .map_err(|e| format!("{locale:?}: no plural rules: {e}"))?;
    let mut seen = Vec::new();
    for n in 0..=1000_u32 {
        let category = rules
            .select(n)
            .map_err(|e| format!("{locale:?}: plural rules failed on {n}: {e}"))?;
        if !seen.contains(&category) {
            seen.push(category);
        }
    }
    Ok(seen.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_names() {
        for ok in ["en", "fr", "pt-br", "zh-hans", "zh-hant", "sr-latn"] {
            assert!(check_locale_name(ok).is_ok(), "{ok}");
        }
        for bad in ["pt_br", "pt-BR", "zh-Hans", "PT", "", "not a locale"] {
            assert!(check_locale_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn plural_form_counts() {
        for (locale, n) in [
            ("en", 2),
            ("fr", 2),
            ("ja", 1),
            ("vi", 1),
            ("ru", 3),
            ("pl", 3),
            ("cs", 3),
            ("sl", 4),
            ("ar", 6),
            ("pt", 2),
            ("pt-br", 2),
            ("tr", 2),
        ] {
            assert_eq!(plural_forms(locale), Ok(n), "{locale}");
        }
    }
}

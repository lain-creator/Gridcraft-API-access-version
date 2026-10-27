//! Test-only crate: checks the TSV catalogs in `locales/` ([`tree::check_tree`]), so a malformed
//! row, a placeholder that differs from the English source text or a wrong plural form count fails
//! CI here, not in an app. The format is in `docs/catalog-format.md`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod catalog;
pub mod locale;
pub mod tree;

/// The repository root (where `locales/` lives).
pub fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

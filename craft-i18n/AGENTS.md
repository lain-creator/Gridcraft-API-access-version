# AGENTS.md

Shared translations for the Craft Apps: TSV catalogs in `locales/` and the checker CI runs. The
shared standards live in [craftrules](https://github.com/storytold/craftrules) (`../../craftrules`
on a dev machine): read its `AGENTS.md`, then `standards/never-crash.md`,
`standards/engineering.md` and `standards/licensing.md`.

- **Data, not code.** The lookup code is the `craft-i18n` crate in craft-libs
  ([ADR 0002](docs/adr/0002-runtime-in-craft-libs-data-in-craft-i18n.md)). This repo holds only
  the catalogs and the checker (`crates/craft-i18n-tests`).
- **Catalogs:** the format and every rule CI checks are in `docs/catalog-format.md`; the words in
  `GLOSSARY.md`. Shared rows in `common.tsv` only grow
  ([ADR 0003](docs/adr/0003-shared-catalog-grows-additively.md)); an app changes one with an
  override in its own `<app>.tsv`.
- **Clean room:** every translation is written from the plain meaning of the English. Never consult
  or copy another product's localisation, string table or translated interface.
- **Credit:** new translations and migrated catalogs are recorded in `ATTRIBUTION.md`, including
  AI assistance and whether a native speaker reviewed them.
- **Never crash.** The checker's `lib.rs` denies `unwrap`, `expect`, `panic` and friends; the
  workspace forbids `unsafe`.
- **Before merging:** `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`.

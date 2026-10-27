# Lookup code lives in craft-libs; craft-i18n holds only data

The catalog parser, lookup, plural rules (`intl_pluralrules`), locale matching and pseudo locale
live in a `craft-i18n` crate in craft-libs. This repository holds only catalogs, docs and the CI
that checks them. Apps embed the catalogs at build time from `CRAFT_I18N_DIR`, as they do with
craft-fonts, and never list this repository in `Cargo.toml`.

Each app had its own copy of the i18n module, and the copies drifted (WordCraft's and PdfCraft's
`mod.rs` differ by ~3.4k lines). Apps already depend on craft-libs as a pinned git dependency, so
the code there costs no new dependency. The data stays optional: a build without
`CRAFT_I18N_DIR` is English-only but still builds offline, in a fork or on a mirror.

## Considered options

- **Code per app** (status quo): rejected, it is what drifted.
- **Code in this repository as a git dependency**: rejected, it adds a second git dependency and
  makes the data repository mandatory for every build.

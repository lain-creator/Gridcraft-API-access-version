# Using craft-i18n in a Crafting App

craft-i18n is a **fully optional** source of translations. An app must build, test and run without
it, in English; with it, the app embeds the catalogs in `locales/`. The code that reads them is the
`craft-i18n` crate in craft-libs, which apps already depend on
([ADR 0002](adr/0002-runtime-in-craft-libs-data-in-craft-i18n.md)).

## Why not a Cargo dependency

Cargo resolves every dependency in `Cargo.toml`, optional ones included. A git dependency on this
repository would make every build (offline, in a fork, on a mirror) need it. So apps never list it
in `Cargo.toml`; the build option is an environment variable instead, as for craft-fonts.

## The build option: `CRAFT_I18N_DIR`

```sh
git clone https://github.com/storytold/craft-i18n ../craft-i18n
CRAFT_I18N_DIR=$PWD/../craft-i18n cargo build --release
```

The app's `build.rs` writes one static into `OUT_DIR`, always, so the app needs no `cfg`:

```rust
/// (locale, shared catalog, this app's catalog), one entry per locale; "" for a missing file.
pub static CRAFT_I18N: &[(&str, &str, &str)] = &[
    ("fr", include_str!("/abs/craft-i18n/locales/fr/common.tsv"), include_str!("/abs/craft-i18n/locales/fr/wordcraft.tsv")),
    // …
];
```

- **Unset** (the default): the static is empty. The app is English-only.
- **Set to a checkout** (it has a `locales/` directory): one entry per locale that has this app's
  own catalog (`locales/<locale>/<app>.tsv`), embedded with `include_str!` on absolute paths. A
  locale with only shared strings is left out, so an app never offers a language it has barely
  been translated into.
- **Set but not a checkout:** `cargo::warning`, and the static is empty.
- **`CRAFT_I18N_REQUIRED=1`** with a missing or invalid checkout: `cargo::error`. Releases set it,
  so a release never ships English-only by accident.
- `cargo::rerun-if-env-changed` for both variables, and `cargo::rerun-if-changed` for every
  embedded file and the `locales/` directory.
- Web (wasm) builds embed the same locales as desktop.

Use an **absolute** path: Cargo runs build scripts in the crate's own directory.
Write each path into the generated source with `{:?}` (as craft-fonts' `include_bytes!({:?})`), so
Windows backslashes are escaped, and don't `canonicalize` it (on Windows that gives `\\?\C:\…`).

## What the app shows

The app passes `CRAFT_I18N` to the craft-libs crate, with the codes of its own language list: only
those are offered (an embedded locale the app isn't ready for, such as a right-to-left one, stays a
fallback only). For `pt-br` it looks up: `pt-br` app catalog → `pt-br` shared → `pt` app → `pt`
shared → the English source text. An app catalog's row wins over
the shared one in its own locale only. A row the crate doesn't understand (a newer `@` context) is
skipped and logged, and that string shows in English.

## Stale and missing strings

A missing translation never blocks anything: not a feature PR, not a pin bump, not a release.
The string shows in English.

craft-i18n can't see app code. With `CRAFT_I18N_DIR` set, each app prints a report of the English
source texts that have no translation, and the translations of source texts the app no longer
uses. The report always passes.

## Adding strings

A feature PR that adds or changes interface text adds the English only, and merges on its own
schedule. Translators add the other languages here, in craft-i18n PRs, whenever they get to them.
The new strings show in English until the app's [pin](../GLOSSARY.md) reaches the craft-i18n
commit that has their translations.

- **Before the app's cutover** (its branch still has `i18n/*.tsv`): add rows there as today, then
  write them into craft-i18n with the sync script, run from a craft-i18n checkout:

  ```sh
  tools/sync_app.py ../wordcraft <SHA last synced> --ref my-branch
  ```

  The last synced SHA is the app's line in `ATTRIBUTION.md`. The script applies what the branch
  changed since then: new and corrected rows go to `locales/<locale>/<app>.tsv` (never
  `common.tsv`; a changed shared string becomes an override there), deleted rows leave it, and a
  new locale gets its lowercase BCP 47 name and its `@id language.name` row. A row the branch
  didn't touch keeps craft-i18n's wording. It prints the app SHA it read (record it in
  `ATTRIBUTION.md`) and checks the round trip: every row the app has at `--ref` must resolve to
  the same text from `locales/`. `--check` runs only that check. Re-running changes nothing.
- **After cutover** (the app has no `i18n/*.tsv`): add rows straight to
  `locales/<locale>/<app>.tsv`. The app's stale and missing report, run with `CRAFT_I18N_DIR`
  set, lists the source texts with no translation (its missing bucket).

The cutover itself: a final sync against the app's `main`, that craft-i18n PR merges, the app's
wiring PR merges (deleting `i18n/*.tsv` and pinning a craft-i18n SHA that holds the sync), then the
sync runs again against the merge's first parent for rows that landed in between.

## Release builds

Whether craft-common-actions' release workflow checks out craft-i18n and sets
`CRAFT_I18N_DIR` and `CRAFT_I18N_REQUIRED=1`, as it does for craft-fonts, is a pending maintainer
decision.

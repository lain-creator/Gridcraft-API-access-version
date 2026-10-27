# craft-i18n roadmap

> **Last updated:** 2026-10-22 · **Change:** minor (coordination with storytold/craft-libs#198)

Shared translations for the storytold Crafting Apps: one catalog per locale for shared strings,
one per app, checked in CI and embedded by each app at build time. Vocabulary is in
[`GLOSSARY.md`](GLOSSARY.md); decisions are in [`docs/adr/`](docs/adr/).

## Milestones

| # | Milestone | Where | Status |
|---|---|---|---|
| 1 | TSV catalog format and strict checker; Fluent and the unusable library crate removed; translator and integration docs | this repo | done (PR #1) |
| 2 | `craft-i18n` runtime crate: parser, lookup, fallback, plurals (`intl_pluralrules`), locale matching, pseudo locale | craft-libs | done: merged upstream as [storytold/craft-libs#167](https://github.com/storytold/craft-libs/pull/167) (2026-10-18) |
| 3a | Migrate WordCraft and PdfCraft (proves the runtime on the largest catalogs; seeds the shared catalog) | apps | catalogs moved here; app wiring next |
| 3b | Migrate the other TSV apps: PhotoCraft, VectorCraft, FilmCraft, CADCraft, craft-launcher | apps | catalogs moved here (shared catalog grows additively, [ADR 0003](docs/adr/0003-shared-catalog-grows-additively.md)); app wiring next |
| 3c | Convert and migrate FigCraft (TOML), LightCraft (JSON), DesignCraft, GridCraft, EffectCraft (Rust tables) | apps | not started |
| 3d | Add translation to DeckCraft and SoundCraft | apps | not started |

Each app migration: confirm the app is MIT OR Apache-2.0, move its catalogs here (lowercase BCP 47
names, origin recorded in `ATTRIBUTION.md`), add the `build.rs` recipe from
[`docs/integration.md`](docs/integration.md), switch to the runtime crate, delete the app's copied
i18n module, and add the app-side stale/missing string report.

Known migration fixes: PdfCraft's Turkish `@plural` rows listed one form, CLDR (and CI) need two;
done in 3a by repeating the form (Turkish nouns stay singular after numerals). The runtime crate
must pin the same `intl_pluralrules` version as the checker, or their plural form counts can disagree.
storytold/craft-libs#210 moves the crate to `icu_plurals`; the checker follows when it merges, with a
test that the two agree on every locale's form count.

Done in 3b, format only (no wording changes): Hungarian and Turkish `@plural` rows with one form
got it repeated (PhotoCraft, VectorCraft); VectorCraft's Korean and CADCraft's Chinese kept one
form; CADCraft's `\P` is escaped as `\\P`. CADCraft's `@plural` sources were the singular only;
they are now `one|other`, so **CADCraft's `count()` must look plurals up by `one|other`** when it
is wired. PhotoCraft's `sr` catalog is Latin script and moved to `sr-latn`.

Known homograph, not yet fixed: WordCraft's Paragraph Spacing menu has an "Open" preset
(`crates/ui-egui/src/ribbon.rs`, the spacing next to Tight and Relaxed). It shares the key
`("", "Open")` with File > Open, so every locale shows the verb (`es` Abrir, `de` Öffnen). The fix is
app-side in WordCraft's 3a wiring: give the preset a context (`spacing`) and add rows with context `spacing` to
`wordcraft.tsv`. Found by FredApps/craftlingo#1.

## Related work

aiongg's language-support plan, [storytold/craft-libs#198](https://github.com/storytold/craft-libs/issues/198)
(fonts in [#197](https://github.com/storytold/craft-libs/issues/197)), extends the runtime crate
(scripts, isolation, number formatting, file sizes, sorting) and lists moving the apps onto
craft-i18n as this roadmap's milestones 3a–3d. Its pilots delete PdfCraft's and CADCraft's own
Arabic reordering, so those two are wired last, after they move to its patched egui.

Known clash for 3c: aiongg's DesignCraft pilot rewrites the app's 1,010 right-to-left call sites,
and 3c converts DesignCraft's Rust tables. The order is decided when 3c starts.

## Apps

As surveyed on 2026-10-16 (VectorCraft recounted 2026-10-18). Locale counts exclude English.

| App | Format today | Locales | Plurals | Migrated |
|---|---|---|---|---|
| WordCraft | TSV | 22 | none | catalogs |
| PdfCraft | TSV | 22 | `@plural` | catalogs |
| PhotoCraft | TSV | 20 | `@plural` | catalogs |
| VectorCraft | TSV | 22 | `@plural` | catalogs |
| FilmCraft | TSV | 11 | none | catalogs |
| CADCraft | TSV | 12 | `@plural` + own rules | catalogs |
| craft-launcher | TSV | 10 | `@plural` | catalogs |
| FigCraft | TOML | 14 | CLDR | no |
| LightCraft | JSON | 13 | minimal | no |
| DesignCraft | Rust tables | 11 | own rules | no |
| GridCraft | Rust `match` | 9 | none | no |
| EffectCraft | Rust tables | 10 | none | no |
| DeckCraft | none | 0 | – | no |
| SoundCraft | none | 0 | – | no |

## Open decisions

- **Release builds** (maintainer): whether craft-common-actions' release workflow checks out
  craft-i18n at a pinned ref and sets `CRAFT_I18N_DIR` and `CRAFT_I18N_REQUIRED=1`, as it does for
  craft-fonts. Until then, releases are English-only unless an app sets them itself.
- **Repository home**: transferred to storytold/craft-i18n (2026-10-21).
- **Progress-docs shape**: align this page with the craftrules progress-docs standard once it is
  available to this repository.

## Change log

| Date | Change | Summary |
|---|---|---|
| 2026-10-16 | major | First roadmap: milestones 1–3d, per-app survey, open decisions |
| 2026-10-17 | minor | Milestones 1 and 2 done; WordCraft and PdfCraft catalogs migrated (196 shared strings, 28 locales) |
| 2026-10-17 | patch | Milestone 2 runtime crate opened upstream as storytold/craft-libs#167 |
| 2026-10-18 | patch | Recorded WordCraft's "Open" spacing preset translated as the verb |
| 2026-10-18 | minor | Milestone 3b catalogs migrated: PhotoCraft, VectorCraft, FilmCraft, CADCraft, craft-launcher (1279 shared strings, 32 locales) |
| 2026-10-18 | patch | Milestone 2 done: storytold/craft-libs#167 merged after review; app wiring unblocked |
| 2026-10-22 | patch | Repository transferred to storytold/craft-i18n |
| 2026-10-22 | minor | Related work: storytold/craft-libs#198; PdfCraft and CADCraft wired last; plural engine follow-up; DesignCraft clash noted for 3c |

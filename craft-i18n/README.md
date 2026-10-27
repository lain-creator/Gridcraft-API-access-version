craft-i18n
==========

Shared translations for the storytold Crafting Apps.

The apps share a lot of strings. Keeping them here means each one is translated once, and every
app picks it up. An app can still override a shared string, or add its own, in its own catalog.

## What's here

| Path | What |
|---|---|
| `locales/<locale>/common.tsv` | Shared strings: those two or more apps use. |
| `locales/<locale>/<app>.tsv` | One app's strings, and its overrides of shared ones. |
| `docs/catalog-format.md` | The catalog format, the rules CI checks, and the header every catalog starts with. |
| `docs/integration.md` | How an app embeds the catalogs as a fully optional build input. |
| `docs/adr/` | Why TSV keyed by English, and why the lookup code lives in craft-libs. |
| `GLOSSARY.md` | The words this project uses: source text, context, catalog, override… |
| `ROADMAP.md` | Milestones and which apps have moved here. |
| `ATTRIBUTION.md` | Translators, and where migrated catalogs came from. |
| `AGENTS.md` | Instructions for coding agents, pointing at the shared craftrules standards. |
| `crates/craft-i18n-tests` | The catalog checker CI runs. |

## How the apps use it

**Fully optional, never required.** Same rule as craft-fonts: the apps don't list craft-i18n in
`Cargo.toml`. Building with `CRAFT_I18N_DIR=<absolute path to this checkout>` embeds these
catalogs; without it, the app builds and runs in English. The lookup code (parsing, fallback,
plurals, the system locale) is the `craft-i18n` crate in craft-libs. See
[`docs/integration.md`](docs/integration.md).

## Translate

Catalogs are plain text, one string per line: `context<TAB>English<TAB>translation`. To add or
fix a translation, edit `locales/<locale>/<file>.tsv` (GitHub's web editor is fine) and open a pull
request. The English is exactly what the app shows; copy it from another locale's catalog of the
same name. Partial catalogs are welcome: anything missing shows in English.

Until an app's cutover it still reads its own `i18n/*.tsv`: translate that app there, in its own
repository, and the rows reach this repo with `tools/sync_app.py`
([`docs/integration.md`](docs/integration.md)). [`ROADMAP.md`](ROADMAP.md) lists which apps have
cut over.

**Clean room:** write every entry from the plain meaning of the English text. Don't consult or copy
any other product's localisation, string table or translated interface.

The full rules (contexts, placeholders, plurals) are in
[`docs/catalog-format.md`](docs/catalog-format.md); CI explains any row it rejects, by file and
line.

## Develop

```sh
cargo test --workspace                          # check every catalog
cargo test coverage_report -- --nocapture       # rows per catalog against the largest locale
```

Code and translations are MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

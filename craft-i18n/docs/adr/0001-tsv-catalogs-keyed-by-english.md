# TSV catalogs keyed by English source text, not Fluent

craft-i18n stores translations in the TSV catalog format the Crafting Apps already use
(`context<TAB>English source<TAB>translation`), keyed by the English text as written in the app's
`tl!(...)` call, instead of Fluent (`.ftl`) files keyed by message ids.

Seven storytold apps (WordCraft, PdfCraft, PhotoCraft, VectorCraft, FilmCraft, CADCraft,
craft-launcher) already ship this format, about 20k strings across up to 22 locales. Fluent would
have meant rewriting every catalog and every `tl!` call site to invent ids, for selector features
the TSV format partly covers already (`@plural` rows). The apps that use other formats (FigCraft's
TOML, LightCraft's JSON, Rust tables in DesignCraft, GridCraft, EffectCraft) convert to TSV, since
it is the majority format.

## Consequences

- Changing an English source text in an app orphans its translations in every locale; app-side
  reports catch this (craft-i18n cannot see app code).
- Same English, different meaning needs a context, not a new id.

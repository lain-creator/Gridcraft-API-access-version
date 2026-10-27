# Catalog format

A catalog is one locale's translations of a set of strings: `locales/<locale>/common.tsv` for
shared strings, `locales/<locale>/<app>.tsv` for one app's strings and its overrides of shared
ones. CI (`cargo test --workspace`) checks every rule below.

## Rows

One string per line, three columns separated by a TAB:

```
context <TAB> English source text <TAB> translation
```

- **Source text** is the English exactly as the app writes it (`tl!("Save")`). It is the key:
  English is never translated, so there are no English catalogs.
- **Context** is usually empty. A plain word (`menu`, `preview`, `cameraRaw`) is a label that tells
  apart two meanings of the same English; the app falls back to the plain row. Contexts starting
  with `@` mean something to the apps, and only these exist:
  - `@plural`: a string chosen by a count (below).
  - `@id`: the source is a command id, not English; the translation is the command's label.
  - `@msg`: a longer message (errors, status text); checked like a plain row. With a `{name}`
    placeholder it is a template that matches any text there, and CI rejects one the apps could
    never match: a `{` with no `}`, two placeholders in a row (`{a}{b}`), or no literal text at all
    (`{x}` alone). The apps skip such a row and show English.
- **Escapes** inside a column: `\n`, `\t`, `\\`. No others.
- `{name}` **placeholders** must appear in both the source text and the translation, in any order.
- A trailing `…` in one needs one in the other.
- An empty translation is an error: leave the row out and the app shows English.
- Each (context, source text) appears once per catalog.
- Lines starting with `#` and blank lines are ignored. Windows line endings and a byte-order mark
  are fine.

## Language name

Each `common.tsv` names its language in that language, for the apps' language pickers:

```
@id	language.name	Français
```

Without it, a picker shows the locale code, so CI requires the row in every locale, in its
`common.tsv` or an app's catalog.

## Plurals

An `@plural` row's source text is `one|other` English forms, both with the same placeholders. Its
translation lists the locale's plural forms, `|`-separated, in CLDR order (`zero`, `one`, `two`,
`few`, `many`, `other`, leaving out those the language doesn't use). The number of forms is the
CLDR cardinal categories whole numbers 0 to 1000 reach: 1 for Japanese, 2 for French, 3 for
Russian, 6 for Arabic.

```
@plural	{n} page|{n} pages	{n} page|{n} pages
```

## Locales and files

- Locale directories are lowercase BCP 47: `fr`, `pt-br`, `zh-hans`, `sr-latn`.
- Files are `common.tsv` or `<app>.tsv`, with `<app>` in lowercase letters, digits and `-`.
- Files starting with `.` are ignored; anything else is an error.
- An app row with the same context, source text **and** translation as `common.tsv` is an error:
  that override changes nothing.
- A row of a regional locale (`es-mx`) that gives the same translation as its base language
  (`es`) is an error: the app falls back from region to base, so the row changes nothing. The base
  is compared as the app sees it, its app row, else its shared one. This applies where the app
  really falls back: a locale whose last subtag is a region (two letters or three digits) and
  whose base language has a directory here. Script variants (`sr-latn`, `zh-hans`, `zh-hant`) never
  fall back, since `sr-latn` to `sr` would show Cyrillic, and `pt-br` falls back only if `pt/` exists.
- Partial catalogs are welcome. `cargo test coverage_report -- --nocapture` prints each catalog's
  row count against the largest catalog of the same name.

## What an app shows

For `pt-br`, an app looks up: `pt-br/<app>.tsv` → `pt-br/common.tsv` → `pt/<app>.tsv` →
`pt/common.tsv` → the English source text. A missing translation never errors.

## Header

Every catalog starts with this header (CI requires the `# Clean room:` line):

```
# <Language> (<locale>) interface catalog for <every Crafting App | AppName>.
# Columns, separated by a TAB:  context <TAB> English source text <TAB> translation
#   context  usually empty. `@plural`, `@id` and `@msg` mean something to the apps; any other
#            word is a label that tells apart two meanings of the same English.
#   escapes  \n  \t  \\  inside a column. `{name}` placeholders must appear in both columns (in any
#            order), and a trailing "…" in one needs one in the other.
#   @plural  English `one|other`; the translation lists this language's plural forms, `|`-separated.
#   Lines starting with # and blank lines are ignored. A missing entry shows in English.
#
# Clean room: every entry was written from the plain meaning of the English text in ordinary
# computing vocabulary. No other product's localisation, string table or translated UI was
# consulted or copied; add entries the same way.
```

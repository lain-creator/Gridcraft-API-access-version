# The shared catalog grows additively

From milestone 3b on, migrating an app never changes an existing `common.tsv` row. Where the new
app translates a shared string differently, its wording becomes an override in its own catalog.
New shared strings are promoted only from strings the existing shared catalog doesn't hold yet:
used by two or more apps and translated alike in most locales they share, the translation most
apps use winning. Rows already shared were reviewed in 3a, and re-seeding would churn every
app's overrides each time an app moves.

This narrows the spec's "most-used translation wins" (user story 41): for a string shared before
an app migrated, the earlier apps' wording stays even when later apps agree on another.

## Consequences

An app can gain a shared translation for a string it showed in English in that locale. Where the
app translates that string differently from the apps the shared row came from, in most locales
both translate, the app keeps an override holding its English instead, so a homograph never
shows another app's meaning.

## Amendment: `@msg` rows are never promoted (2026-10-18)

An `@msg` row is a template that matches runtime messages, not a fixed string. A generic one such
as `{path}: {e}` in `common.tsv` would match other apps' messages, so `@msg` rows stay in each
app's catalog even when two or more apps share them. `Couldn't open {name}: {e}`, promoted in 3b,
stays shared: existing rows never change, and it is specific enough not to over-match.

## Considered options

- **Re-seed `common.tsv` from every migrated app at each migration**: rejected, winners can flip,
  so WordCraft's and PdfCraft's overrides churn and the reviewed shared strings are reviewed again.

# Glossary

**Crafting App**: one of the storytold applications that share these translations (WordCraft,
PdfCraft, PhotoCraft, ArtCraft, …).

**Locale**: a language (optionally with region) that strings are translated into.

**Source text**: the English text of a string, as written in the app. It is also the string's key:
English is never translated, only looked up.

**Context**: an optional label that tells apart two strings with the same source text but
different meanings (e.g. "Open" the verb vs. "Open" the state).

**Catalog**: one locale's translations of a set of strings.

**Shared string**: a string used by more than one Crafting App, translated once in the shared
catalog.

**App string**: a string used by one Crafting App only, translated in that app's own catalog.

**Override**: an app catalog's translation of a shared string; the app uses it instead of the
shared one, in that locale only.

**Translation**: a locale's text for a source text (and context).

**Plural forms**: the variants of one translation chosen by a count (e.g. "1 page" / "2 pages");
each locale has as many as its language's plural rules need.

**Pseudo locale**: a fake locale that marks every translated string, used to find text that
bypasses translation. Never shipped as a choice to users.

**Pin**: the craft-i18n version a Crafting App's release embeds. The app moves it forward on purpose,
never automatically. "Pin" alone always means this one.

**Runtime pin**: the craft-libs version a Crafting App looks its translations up with. It decides how
translated text is shown (numbers, plural choice, direction marks), so moving it can change what
users see even when the pin stays put.

**Offered locale**: a locale a Crafting App lets its users choose. A locale the app has catalogs
for but doesn't offer (one its interface isn't ready for) still serves as a fallback.

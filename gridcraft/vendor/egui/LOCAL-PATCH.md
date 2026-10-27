# Local changes to egui 0.36.2

Source: crates.io `egui` 0.36.2, upstream <https://github.com/emilk/egui/tree/0.36.2/crates/egui>.
License: MIT OR Apache-2.0. Original source and notices retained.

- `Button::new`: explicitly center contents on both axes instead of inheriting the parent
  layout. Menus still use grow atoms to separate labels and keyboard shortcuts.
- `TextEdit::singleline`: vertically center text and caret; multiline editors remain top-aligned.
- `Cargo.toml`: license include paths refer to the copies in this vendored crate.

Regression coverage: `crates/ui-egui/tests/chrome_alignment.rs` in GridCraft.

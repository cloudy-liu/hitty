# Document review controls

Tracks cloudy-liu/ctty7#68, cloudy-liu/ctty7#69, and cloudy-liu/ctty7#70.

Code and diff headers expose a fill/restore button. Filling keeps the session
sidebar and any open right panel outside the document. Restoring uses the saved
split width. The choice belongs to the active session tab.

The diff header uses one layout button. Red and green panes show the current
arrangement; the tooltip names the next layout. Clicking switches the existing
saved diff preference without reopening the patch.

When the central workspace cannot fit the document actions beside the window
controls, the document header moves below the window controls. The sidebar stays
visible and the layout actions remain reachable.

## Component dependency

The UI components and assets come from the maintenance fork
`cloudy-liu/gpui-component`, pinned together at
`2c3c878421d82889d5024f574b22086f483a7d56` in the manifest and lock file.
[Component PR #2](https://github.com/cloudy-liu/gpui-component/pull/2) adds
view- and revision-bound selection snapshots so a toolbar press and document
reflow retain Markdown's selected text and highlighted byte ranges. The app
keeps using its existing locked GPUI runtime from `l0ng-ai/zed`.
[Component PR #3](https://github.com/cloudy-liu/gpui-component/pull/3) prevents
restored partial selections from locking the inline state twice during painting.
The pinned repair builds on the previous selection snapshot revision and changes
only inline selection painting and its regression test.

## Automated checks

```powershell
cargo fmt --all -- --check
cargo test --locked --bin tty7-app ui::document_column::gpui_tests
cargo test --locked --bin tty7-app ui::markdown_preview::tests::fill_and_restore_preserve_a_partial_markdown_selection
cargo build --locked
cargo test --locked
```

The GPUI tests click the rendered controls and check:

- Code and diff fill/restore in light and dark themes, retaining sidebar and
  file-tree bounds and restoring the chosen document width.
- The existing right-panel entry stays in place and opens the panel while the
  document fills the workspace.
- The diff layout button switches both ways, retains the focused file, loaded
  patch and expanded sections, and is absent from ordinary code views.
- Layout controls work at 560px and 360px window widths.
- Other session tabs and temporary narrow-window fallback retain their layout.
- Unsaved editor text and cursor position, plus Markdown selection and reading
  position, survive clicks on fill and restore.

The Markdown regression double-clicks a word, clicks the rendered fill and
restore controls, and completes a draw after each change while checking the
selected text. A state-only selection snapshot test does not exercise the
inline painting path where the restored selection previously deadlocked.

## Desktop check

1. Open a file with the left session sidebar and right file tree visible. Click
   fill, then restore. Check that the side panels retain their bounds and the
   document returns to its original split width.
2. Repeat with a diff, switching its layout in both directions. Check the focused
   file and expanded sections remain selected, and reopen a diff to check the
   saved layout preference.
3. Hide the right panel, fill a document, and open the panel through the existing
   titlebar button. Check that the app menu and window controls remain usable.
4. Keep an unsaved edit or a partial Markdown selection, then fill and restore.
   Check the edit, selection, and agent session remain intact.
5. Repeat in light and dark themes and at narrow widths. Check the tooltip names
   the next action, icons align, and the document controls remain clickable below
   the window controls when they cannot share one row.

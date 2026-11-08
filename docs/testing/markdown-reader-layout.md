# Markdown reader layout verification

Verified on Windows on 2026-10-15. The component dependency is pinned to
`cloudy-liu/gpui-component@a9f601a1a632c753ae279a9c25f629d59908d87c`.

## Regression evidence

Before this change, the small-document reader's scrollbar viewport ended at the
panel edge. The virtualized reader's scrollbar was inside the paper padding and
centered content cap. With a 1600px window and Files visible, the viewport edge
was 32px inside a docked reader and 164px inside a filled reader.

`markdown_scrollbar_drags_at_the_panel_edge_for_small_and_large_documents`
failed on the old reader when dragging the long document at the panel edge.
The scroll offset remained unchanged. It passes with the new reader across
short and virtualized documents, docked and filled layouts, and 1100px/1600px
windows. Each case verifies:

- The outer scrollbar overlay ends 2 logical pixels before the Files divider.
  The native thumb's own 4px inset leaves a painted 6px gap.
- The heading follows the responsive padding or the existing centered width cap.
- Pointer dragging changes the document offset without resizing Files.
- Clicking the bottom of the scrollbar reaches the document's maximum offset.

The adaptive-table wrapping test previously measured an 87px table with a long
description at 360px width. Its assertion that cells wrap beyond 120px failed.
It now passes, along with widening/reflow and complete English/CJK selection.
Shift-wheel previously left the horizontal offset unchanged. It now moves the
table and makes the final column's link clickable.

## Component coverage

The component library suite passes 309 tests with the application's GPUI pin
and dependency versions. Added coverage includes:

- Wrapped cell content and selection after resize.
- Horizontal fallback for twelve columns and activation of the final link.
- A 1000-row table in a virtualized document: visible-row painting, width-driven
  row-height invalidation, the final link after scrolling to the end, and full copy.
- A 100-row HTML table inside document item 5: the document's cached scroll range
  follows the table's changed height after resize, and its final link is reachable.

The 1000-row test recorded approximately 9.2ms per warm scroll frame over forty
frames. This measures the GPUI test backend's CPU layout/prepaint/paint path.
It excludes GPU execution, presentation and display refresh. Native GPU frame
rate and macOS/Linux behavior remain unverified.

## Application suite

Run the final pinned dependency without a local path patch:

```powershell
cargo test --workspace --locked
```

On Windows, Git's `bin` and `usr/bin` directories must be on `PATH` for the shell
integration tests. Debug information and optimization overrides may be used to
reuse an existing local build cache; they do not change dependency resolution.

The full application test binary passes 1724 tests, with six existing ignored
tests, when run with `--test-threads=1`. The initial parallel workspace run had
one failure in `agent_sound_settings_persist_without_changing_desktop_policy`:
its reloaded sound mode was `Unfocused` instead of `Always`. That test passes
both in isolation and in the complete serial application run. Its scratch
configuration directory is shared by all tests in the same process.

The remaining workspace packages pass 1351 tests, with four existing ignored
tests, under `cargo test --workspace --exclude tty7 --locked`. This includes
thirteen cases in the CLI's custom end-to-end harness. The two successful runs
cover all workspace packages with the final Git pin and no local path patch.

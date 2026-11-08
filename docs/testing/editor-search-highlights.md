# Editor search highlights

The current search result must be distinguishable from other matches, including
when the application's selection or accent color is gray. The reported case
showed `5/16` while every `icon` match had the same `#434750` background.

The code editor supplies independent search colors through
`gpui-component::input::CodeEditorStyle`. Ordinary matches use a neutral fill;
the current match uses the active application accent. Both fills reuse the
terminal's contrast targets, calculated from the resolved editor background
and foreground. The fills are opaque, so painting the current match over an
ordinary match does not change its final color.
The editor's text selection, caret and syntax palette are preserved. The theme
resolver includes these search colors in the complete style passed to `Input`
once, so a later theme setter cannot overwrite the search visibility fix.

The component and asset packages are pinned together to
`cloudy-liu/gpui-component@a8bc7a71b974cbf190b86622837e46e4508070e7`.
This existing component revision supplies the instance color interface.

## Automated regression coverage

```powershell
cargo test --locked --bin tty7-app editor_search_highlights -- --nocapture
cargo check --locked
cargo test --locked
cargo fmt --all -- --check
```

The regression test exercises the final editor theme resolver and fails with
the bundled search fills that would overwrite the search visibility fix.
It checks the automatically resolved Atom One palette for every built-in application
theme with colored, gray and editor-background-colored accents. One Dark Pro and
other dark application themes use Atom One Dark; light themes use GitHub Light.
It requires at least 1.2:1 luminance contrast between the two match fills,
at least 1.89:1 between the
current match and the editor background, and at least 2.95:1 between the
editor's body text and each fill. It also checks opaque fills and preservation
of the selection, caret and syntax palette. These regression bounds do not
claim that all syntax token colors meet a text accessibility standard.

## Manual verification

Open a source file containing several occurrences of `icon`, then press
`Ctrl+F` on Windows or Linux, or `Cmd+F` on macOS. Search for `icon` and move
forward and backward through the results using Enter, Shift+Enter and the
search panel arrows. Check that one match uses the accent fill, its position
follows the counter, and the remaining matches use the neutral fill.

Repeat with light and dark application themes, including One Dark Pro, Dracula
and Nord. Switch themes with the search open, and select some source text to
check its usual selection color. Automated
color tests do not replace this native window inspection, especially with
custom image or gradient backgrounds.

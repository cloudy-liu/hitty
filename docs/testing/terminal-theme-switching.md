# Terminal live theme switching

Applications can retain startup RGB or indexed colours across a terminal
theme change. The renderer handles this without inspecting the application
name or matching a list of known colours.

With `theme_legible_palette` enabled, fills whose light/dark appearance
conflicts with the canvas move near its lightness. Hue and chroma are retained
within the RGB gamut. Text contrast is measured on the resolved fill, including
SGR dim. Same-appearance fills and already readable text are retained.
Explicit foreground colours on Unicode Block Elements (`U+2580`–`U+259F`)
use the background mapping too. TUIs use these characters to extend fills
across partial rows; text contrast correction would turn them into bright
stripes. Blocks using the default foreground retain text contrast correction.
Disabling the setting renders application colours literally.

The adapter caches colour calculations for one frame. It never changes the
terminal grid, so theme round trips and scrollback have no cumulative drift.

## Automated checks

```sh
cargo test --locked -p tty7 --bin tty7-app terminal::
```

The renderer tests replay Cursor Agent and Codex SGR samples alongside arbitrary
RGB, indexed and named ANSI fills without providing any agent identity. They
apply dark, light and dark themes to the same grid and check backgrounds,
composited text contrast, literal mode, selection, inverse and concealed text,
pane dimming, and unchanged source cells.
The Cursor Agent v2026.10.14 prompt regression replays its `#2f3239` foreground
half blocks and matching SGR background. It checks that both edges match the
input fill across dark/light/dark theme changes. Additional cases cover all
block elements, RGB/indexed/named colours, literal mode, and readable text.

Colour-math tests sweep all greys and small red/green/blue tints over multiple
custom canvases. They check appearance, chroma, native-colour preservation and
saturated-colour gamut bounds, rather than accepting only captured RGB values.

## Manual check

1. Start a TUI in a light theme and leave its session running.
2. Display an input box, submitted text and coloured output such as a diff.
   In Cursor Agent, check that the half-row padding above and below the prompt
   matches its background rather than appearing as two bright stripes.
3. Switch to a dark theme. Check the backgrounds, text, placeholder and cursor.
4. Switch back to light and then dark. Scroll through older output.
5. Repeat with the application started in dark mode and inside a host
   application. Repeat with an application tty7 does not identify.
6. Disable the colour-correction setting and check that explicit colours return
   to their original values. Re-enable it in the same session.

## Scope

This is a rendering policy, not a refresh of another application's palette
cache. Terminal colours have no semantic metadata; block elements provide a
character-based fill hint, but other deliberately pale/dark artwork is subject
to text contrast correction. Use literal mode when exact colours matter.
The fixed lightness band for remapped soft fills is eight percentage points.
Saturated colours can remain further from the canvas to stay in gamut, and dim
text on midtone surfaces may not reach 4.5:1 even at the best ink endpoint.

# Agent pane titles and hover actions acceptance

Spec: cloudy-liu/ctty7#234. The HTML prototype and its renders under
`docs/prototypes/agent-pane-titles/` are design references; the captures below
are the native application.

## Reproducible native state

Baseline source: `1063bc9b` (fork `main`) with only the fixture overlaid by
`scripts/agent-pane-native-visual.py prepare-before`. Final application source:
`b70928a0` (the feature commit `0dec85b0` plus the borderless toolbar centred on the
title row, the 13px name, the `ctty7` head label for an unnamed workspace, the
name-width fix, and the hover toolbar on every pane). Both are debug builds of the production app with
`--features markdown-visual-tests`, started as
`tty7-app --config-dir <fresh dir> --agent-pane-visual <options.json>`.

Windows 11, 175% display scale, a 1200 x 700 logical window (the settings
scene asks for 1200 x 1400 and the screen clamps it to 923), left sidebar at
240, one tab holding a 2 x 2 split. Three panes report a recognized agent
through the normal daemon message (Claude Code, Codex, Cursor, all `Working`);
the fourth is a PowerShell terminal with no agent. The terminal text is
fixture output; no agent or shell runs. The title setting travels to the
fixture as JSON, which the baseline ignores, so one fixture builds on both.

Captures are `scripts/capture-native-window.ps1`: `PrintWindow` of the fixture
window at physical resolution, no pointer, no overlapping windows. Hover scenes
post one `WM_MOUSEMOVE` to the window a quarter of the way across the pane's
top edge (clear of the drag grip's handle in the middle); the settings scene
posts `WM_MOUSEWHEEL` through `scripts/post-mouse-wheel.ps1` to reach the end
of the Tabs block. The real pointer never moves. Each scene is a fresh process
and config dir, so Before and After differ only in the binary.

## Matched Before and After

**Pointer on the Claude Code pane's top edge, titles off.** Before,
only the existing drag grip appears. After, the same grip plus the four tiles
at the pane's top-right: split right, split down, zoom (outward arrows), close.
The terminal does not move: the toolbar is an overlay.

![Before: hover, titles off](images/agent-pane-titles/before-hover-dark.png)
![After: hover, titles off](images/agent-pane-titles/after-hover-dark.png)

**Titles on, pointer on the Claude Code pane.** After, the three agent panes carry
a 28px row with the agent's icon and name above their first terminal line; the
PowerShell pane does not. The toolbar sits inside the hovered pane's row.

![Before: hover, titles on](images/agent-pane-titles/before-titles-hover-dark.png)
![After: hover, titles on](images/agent-pane-titles/after-titles-hover-dark.png)

**Titles on, no pointer.** The rows alone, no tiles: focus does not reveal them.

![Before: titles on, no hover](images/agent-pane-titles/before-titles-dark.png)
![After: titles on, no hover](images/agent-pane-titles/after-titles-dark.png)

**Pointer on the PowerShell pane's top edge, titles on.** After the user's review
every pane offers the four tiles; the ordinary terminal still gets no title
row. Before, only the pre-existing grip answers the hover.

![Before: hover on the shell](images/agent-pane-titles/before-ordinary-hover-dark.png)
![After: hover on the shell](images/agent-pane-titles/after-ordinary-hover-dark.png)

**Claude Code pane zoomed, pointer on its top edge, titles on.** After, the pane
filling the tab keeps its row and toolbar, with inward arrows for restore.

![Before: zoomed](images/agent-pane-titles/before-zoom-hover-dark.png)
![After: zoomed](images/agent-pane-titles/after-zoom-hover-dark.png)

**Light theme, titles on, with and without hover.**

![Before: light](images/agent-pane-titles/before-titles-light.png)
![After: light](images/agent-pane-titles/after-titles-light.png)
![Before: light, hover](images/agent-pane-titles/before-titles-hover-light.png)
![After: light, hover](images/agent-pane-titles/after-titles-hover-light.png)

**Settings, Window & Tabs, scrolled to the end of the Tabs block.** After, a
"Show agent pane titles" switch follows "Show agent status text in sidebar",
on (the default since the user's review; the spec had proposed off).

![Before: settings](images/agent-pane-titles/before-settings-light.png)
![After: settings](images/agent-pane-titles/after-settings-light.png)

**One wide pane beside a column of three, titles on, no pointer.** The shape in
which review found agent names cut down to an ellipsis: the name element had no
claim on the row's width and was measured at its minimum. Before is the
baseline (no rows). After, "Codex" and "Cursor" read in full in the narrow
column; the fixture's `layout: "column"` option builds this tree.

![Before: column layout](images/agent-pane-titles/before-titles-column-dark.png)
![After: column layout, names in full](images/agent-pane-titles/after-titles-column-dark.png)

**Control: titles off, no pointer.** Both binaries draw the same screen (the
fixture sets the title setting explicitly; the shipped default is on).

![Before: default](images/agent-pane-titles/before-default-dark.png)
![After: default](images/agent-pane-titles/after-default-dark.png)

## LCU captures

The three pairs below were captured again through the LCU native-app surface
(`cua.getApp` on the fixture window, then `getScreenshot`) to satisfy the
repository's capture-tool requirement: same binaries, fixture options and
posted pointer and wheel messages as above, with the real pointer parked at
the screen's top-left corner outside the window before each capture, and none
shows it. LCU returns the window's on-screen region as a JPEG at logical
resolution, so the settings pair shows the part of the clamped 1400px window
that fits on screen; the PNG set above remains the full-resolution record.
Inspect the same things: tiles at the hovered pane's top-right, the rows on
the three agent panes and not on PowerShell, and the new switch after the
sidebar status row. The LCU After frames were captured at `2b10418a`: they
show the borderless toolbar but predate the row alignment, the 13px name and
the `ctty7` head label of the final revision; a re-capture waits for a free
screen, since LCU reads the window's on-screen region.

| LCU Before | LCU After |
| --- | --- |
| ![pointer on Claude Code, titles off: before](images/agent-pane-titles/before-hover-dark-lcu.jpg) | ![pointer on Claude Code, titles off: after](images/agent-pane-titles/after-hover-dark-lcu.jpg) |
| ![pointer on Claude Code, titles on: before](images/agent-pane-titles/before-titles-hover-dark-lcu.jpg) | ![pointer on Claude Code, titles on: after](images/agent-pane-titles/after-titles-hover-dark-lcu.jpg) |
| ![Window & Tabs, Tabs block: before](images/agent-pane-titles/before-settings-light-lcu.jpg) | ![Window & Tabs, Tabs block: after](images/agent-pane-titles/after-settings-light-lcu.jpg) |

## Automated checks

On this Windows machine: the config round trip (`show_agent_pane_titles`
absent, false, true), the chrome decision for agent and non-agent panes, the
split icons resolving to SVG artwork, and the locale parity test over the new
keys. The app-level GPUI tests in `src/ui/pane_title.rs` (top-edge reveal on
the agent pane and never on the shell, the setting adding and removing the row,
a tile acting on its own pane rather than the focused one, an exiting agent
taking its chrome with it) are unix-only like every pane test and run on CI.

## Known limitations

- The LCU After frames lag the final revision by the three cosmetic tweaks
  named above; the PNG set is current.
- Hover is a posted pointer message, not a physical pointer. The reveal path
  it exercises is the production `on_hover` band; a physical pointer takes the
  same path.
- The drag grip's dots show in every hovered split pane in both phases; that
  is existing behaviour, not part of this change.
- No narrow-pane capture: the title row truncates its name with an ellipsis,
  which the fixture's 480px panes do not reach.

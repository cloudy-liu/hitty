# README screenshots

Captured on Windows on 2026-10-23 from the v0.8.0 application renderers at
`b0b573163c0fc3f30b34f57500511d2ef9aa2ad6`, the fork's current `main` at capture
time. This PR extends the opt-in document fixture with a README sample workspace
and gives both visual fixtures the same titlebar options as the production app.
No application renderer was changed to produce these images.

| Image | Native pixels | Contents |
| --- | --- | --- |
| [workspace-light.webp](workspace-light.webp) | 2419 x 1332 | Light workspace with all four lowercase agent tags, a pinned project, a folded group, three file tabs, Markdown and Mermaid preview, and the file tree. |
| [workspace-dark.webp](workspace-dark.webp) | 2419 x 1332 | The same workspace in dark appearance. |
| [editor-light.webp](editor-light.webp) | 2419 x 1332 | Source editing with file tabs, project breadcrumbs, and GitHub Light syntax colors. |
| [html-preview-light.webp](html-preview-light.webp) | 2419 x 1332 | A local HTML page rendered by WebView2, with Preview / HTML, external-browser and refresh controls beside the breadcrumb. |
| [remote-top.webp](remote-top.webp) | 2104 x 1157 | SSH top tabs with blue Remote badges and connection/directory names. |
| [remote-sidebar.webp](remote-sidebar.webp) | 2104 x 1157 | The same SSH sessions in the sidebar, including the connection below task and manual titles. |

[Capture metadata](captures.json) records the production revision, executable
hash, capture times, dimensions, and native/encoded image hashes.
[assets/hero.webp](../hero.webp) and [docs/images/hero.webp](../../docs/images/hero.webp)
are byte-identical copies of `workspace-light.webp` for existing documentation links.

## Capture method

The document fixture opens the full production `Tty7App`. Its sample pane data
travels through the normal daemon message reader. Codex is idle, Claude Code is
working, Gemini needs input, and Cursor has an unread result. These states use
controlled events; no agent CLI runs and no model request is sent. Branch names
and the `+2` counts come from an actual throwaway Git repository prepared by the
script. The nested Playground repository is folded and the main project is pinned.

The SSH fixture uses `auto_names: true` and controlled remote context messages.
It exercises the real tab and sidebar renderers without opening a network SSH
connection. These screenshots do not establish live remote-agent detection.

Local Computer Use selected each fixture window and sent Alt+PrintScreen.
The original Windows clipboard bitmap was saved as PNG at the existing 175%
display scale. Document viewports are 1380 x 760 logical pixels; SSH viewports
are 1200 x 660. The images include the native window border. The HTML capture
includes its native child WebView, which is absent from GPUI GPU-only captures.

All six PNGs were encoded as lossless WebP without resizing, cropping, compositing,
retouching, or generated UI. Decoded RGBA bytes matched their original native PNGs
exactly. No mouse highlight, tooltip, or extra system titlebar covers the UI.

## Reproduce

Build the opt-in visual fixtures and prepare a new scratch directory outside the
checkout. The preparation script uses only Python's standard library and Git;
it refuses to reuse an existing output directory.

```powershell
cargo build --locked --features markdown-visual-tests --bin tty7-app
python scripts/prepare-readme-captures.py D:/gh-prj/ctty7-readme-capture
```

Launch one case at a time with a separate configuration directory. For example:

```powershell
.\target\debug\tty7-app.exe --config-dir D:/gh-prj/ctty7-readme-capture/light-config --document-visual D:/gh-prj/ctty7-readme-capture/workspace-light.json
```

The other document options are `workspace-dark.json`, `editor-light.json`, and
`html-preview-light.json`. Wait for `NATIVE_DOCUMENT_READY_FOR_CAPTURE` and
`"ready": true` in the case's readiness JSON before capturing the window.
The fixture checks agent identities, states and the selected mode; readiness JSON records the open document count.

Use `--ssh-tab-visual` instead of `--document-visual` for `remote-top.json` and
`remote-sidebar.json`; wait for `NATIVE_SSH_TABS_READY`. Each fixture holds its
window for up to ten minutes. On Windows, select the returned native window,
capture it with Alt+PrintScreen and save the clipboard image as PNG. Preserve
the original size when encoding WebP and compare decoded pixels before replacing
an asset. See [document evidence](../../docs/testing/document-tabs-native.md)
for the underlying fixture and [SSH naming evidence](../../docs/testing/ssh-tab-auto-name.md)
for the remote sample cases.

The capture matrix covers Windows light/dark workspaces, source editing, local
HTML, and both SSH tab layouts. macOS and Linux images were not recaptured.

## Historical images

[agents-light-dark.webp](agents-light-dark.webp) and [source-light.webp](source-light.webp)
remain available for earlier release-note links. They were captured on Windows
on 2026-10-07 from `b4f7b086` plus the v0.2.0 version change.

The old images under `docs/testing/images/` and `docs/specs/images/` remain
unchanged as their original acceptance evidence. In particular,
`sidebar-status-uppercase-light.png` and `sidebar-status-uppercase-dark.png`
are the pre-change baseline from the lowercase-tag comparison, not the current
application UI. Neither README references those baseline screenshots now.

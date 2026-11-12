# Hitty product screenshots

Refreshed on November 12, 2026 from `main` at `0a9dedc` on Windows using the existing
`prepare-readme-captures.py` document scenes and `capture-native-window.ps1`
PrintWindow harness. All four refreshed PNGs were inspected. No image is
composited, retouched, or generated; no pointer appears in the captures.

The viewport is 1380 by 760 logical pixels at 175% display scale. The native
frame is 2419 by 1332 pixels. Separate temporary configuration directories and
a throwaway Git repository provide a reproducible workspace.

| Image | Content |
| --- | --- |
| workspace-light.png | Light workspace, project groups, Agent states, Markdown, Mermaid and automatic Outline follow |
| workspace-dark.png | The same scene in dark appearance |
| editor-light.png | Source editing with document tabs and project navigation |
| html-preview-light.png | Native WebView2 rendering of a local HTML page |

Agent states are controlled sample messages passed through the production
renderers. No live Agent CLI, model request, or network SSH connection is used.
These captures demonstrate the interface, not live Agent detection.
The opt-in fixture exits before normal application startup and hook refresh.

`captures.json` records source revision, binary digest, exact scene settings,
readiness, capture output, and each PNG digest. The documentation homepage uses
byte-identical copies of the light and dark workspace images.
The source and HTML scenes explicitly select Files; Markdown follows its outline.

Reproduce with a new scratch directory:

```powershell
cargo build --locked --features markdown-visual-tests --bin hitty-app
python scripts/prepare-readme-captures.py D:/tmp/hitty-readme-scenes
```

Launch `hitty-app --config-dir <isolated-directory> --document-visual <scene.json>`,
wait for the readiness record, then run the PrintWindow script with its process
ID. Retain the native image size. See the inherited harness instructions in
`../README.md` for the complete scene preparation workflow.

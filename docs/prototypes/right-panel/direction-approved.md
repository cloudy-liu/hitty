# Right panel prototype direction

The maintainer selected the Otty-style panel layout during the November 8,
2026 discussion and then approved creating an interactive prototype.

> 这一点我觉得直接可以对齐吧？对齐OTTY那个布局

> 先讨论，然后我们做一个原型，直接通过原型来对齐要改的效果。

The confirmed scope is four modes (Info, Outline, Git, Files), a label on the
active mode, a centered mode selector without the prominent horizontal divider,
and visible hidden-file and refresh controls beside the directory name.
Hitty's existing colorful file icons are retained. Search remains available.
Outline demonstrates terminal commands, Agent prompts, and document headings.

The maintainer answered “同意” to that concrete proposal. This is refinement of
the selected reference layout; unrelated alternative visual directions are not
needed. The prototype is for review before application implementation.

## Scope clarification

> 现有的功能别动。只动布局和外观。

The latest clarification takes precedence: preserve Hitty's information rows,
context actions, file icons, search, Git controls and interaction semantics.
Remove the redundant Files heading and move the existing collapsible root name
into that position. Preserve root folding. Repeat-clicking the active mode
continues to close the panel, as in Hitty's current implementation.

Info keeps session, conversation, process and port content, with remote fields
shown when relevant. Git keeps branch/sync, commit message and split action,
change groups, per-file and group actions, and commit history. Prototype actions
operate on sample data or explain the existing entry point; none run Git or
change real files. Four modes are an approved presentation change. Agent outline
reuses the existing conversation navigation concept.

## Document flow and icon clarification

> 最左侧会话列表不应新增“阅读项目文档”这一项

> 右侧边栏我们改造的这个东西，跟左侧边栏没有任何关系。

> 四个标签的图标大小要对齐 Ottty。

The right-panel redesign must not create, remove, reorder, rename or select a
left-sidebar session. Opening a document creates a document tab within the
current session. The preview keeps the existing application placement behavior.
Document tabs start empty, are created by opening a file, close independently,
and belong to the session that opened them. The sidebar has no document-only
session or fabricated “read project document” item.

The maintainer explicitly confirmed that an open Markdown document should show
its heading hierarchy in Outline immediately, without an extension-preview
switch. The prototype now derives headings from the displayed document and
links them to the corresponding content. Terminal commands and Agent turns are
the other context examples, not operations on the left sidebar. Creating these
new outline providers in the native application is an explicit right-panel
enhancement, not a claim that all of them already exist.

Mode icons use a shared 16 logical-pixel display box and balanced SVG viewboxes,
with separate 30 px control height, based on the provided Otty reference. The
file-tree icon assets and their list sizes are unaffected. Native acceptance
must compare at the same display scale rather than using enlarged screenshots.

## Evidence and limits

- Reference: maintainer screenshots and direct LCU inspection of installed Otty.
- Observed: active-only mode text, four modes, file controls, independent panel
  toggles, left panel resize, file expansion and context menus.
- The inspected Outline was empty (“No commands recorded”). Populated Outline
  behaviors in this prototype demonstrate the agreed Hitty proposal; they are
  not a claim of exhaustive Otty feature verification.
- Hitty already has Agent turn navigation in `src/ui/right_panel.rs`; a separate
  Outline mode is proposed. The prototype does not implement it in the app.

## Asset and presentation specification

Reuse the approved Hitty app icon, UI SVGs from `assets/icons`, and the Symbols
file icons resolved from `assets/file-icons/symbols/symbol-icon-theme.json`.
The Symbols MIT license is embedded in the standalone HTML. No network assets,
new packages, custom display font, or generated brand artwork are required.

The prototype is an operating workspace, not a marketing page. Preserve a
familiar Windows UI font and Hitty's light neutral surfaces. Primary emphasis
belongs to the active task, active mode, and selected file. Use the approved
reference's quiet gray active-mode pill, evenly spaced icons and continuous
right-panel surface. The existing file-list density and file-type colors remain
recognizable. Default view is a complete window so panel proportions can be
judged in context; a focus control also isolates the right panel for inspection.

At a 1440 by 1000 viewport, the review toolbar sits outside the simulated app.
The app contains a sidebar, main content area and resizable right panel. Mode
labels and controls must remain usable at 260–440 px panel widths. Light/dark
switching, tooltips, visible keyboard focus, and reduced-motion support are
included. Terminal, Agent, Markdown and HTML scenes use explicitly labeled
sample content, never live personal sessions or shell commands. File search,
folder expansion, hidden-file filtering, refresh feedback and outline jumps
are local interactions. Implementation details and review explanations stay
outside the simulated application surface.

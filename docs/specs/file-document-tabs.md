# Spec: Multi-file document tabs with breadcrumbs and explicit preview modes

## Problem Statement

Opening a file in ctty7 appears to replace the previous document. Although multiple buffers already exist, the document panel exposes only the current filename and path. Readers cannot see their open files or switch and close them directly. Markdown's bottom preview icon also hides both the available modes and the selected mode.

## Solution

Provide a document workspace within each terminal session. A horizontal file tab strip shows every open document, followed by a separate row containing the active file's path breadcrumbs on the left and its preview/source controls on the right. Markdown opens in Preview by default; supported local HTML opens in Preview too. Each file retains its own editing and viewing state and can be closed independently.

## User Stories

1. As a terminal user, I want each opened file to have a visible tab, so that I can see which documents remain open.
2. As a reader, I want a newly opened file to become active without closing previous files, so that I can follow references and return.
3. As a reader, I want clicking an already open file to activate its existing tab, so that I do not create duplicates.
4. As a reader, I want tabs to display only the file icon, basename, modification indicator and close control, so that filenames are easy to scan.
5. As a reader, I want the active tab to be visually distinct, so that I know which document I am viewing.
6. As a reader, I want the active file's path in a separate breadcrumb row below the tabs, so that directory names do not crowd the tab labels.
7. As a reader, I want a full-path tooltip, so that I can distinguish same-named files and inspect truncated paths.
8. As a reader, I want tab order to stay stable when I switch files, so that I can return by position.
9. As a reader, I want an overflowing tab strip to scroll horizontally and reveal the active tab, so that many files remain accessible.
10. As a Markdown reader, I want a new Markdown file to open in Preview, so that I can read rendered content immediately.
11. As a Markdown author, I want explicit Preview and Markdown buttons on the right of the breadcrumb row, so that I can switch modes and see the current mode.
12. As a local HTML user on a supported platform, I want Preview and HTML buttons in the same position, so that document controls remain consistent.
13. As a code editor user, I want ordinary source files to open in the editor without preview controls, so that irrelevant actions do not occupy space.
14. As an author, I want preview and source to share one file tab, so that changing modes does not duplicate my document.
15. As an author, I want switching tabs to preserve edits, undo history, cursor and source scroll, so that I can continue editing where I stopped.
16. As a reader, I want each file's selected mode and reading scroll position preserved across tab switches, so that I can continue reading where I stopped.
17. As a local HTML reader, I want ordinary page scroll restored when returning to an HTML file, so that switching files does not send me to the top.
18. As a reader, I want each tab's close button to close that file only, so that other documents remain available.
19. As an author, I want closing a modified file to offer Save, Discard and Cancel, so that I do not lose edits.
20. As a reader, I want closing an inactive tab to preserve the current selection and closing an active tab to select a neighbor, so that closing files is predictable.
21. As a terminal user, I want closing the last file to dismiss the file panel, so that I return to my terminal.
22. As a reader, I want the bottom preview toggle removed, so that there is one clear place to change modes.
23. As a reader, I want Preview mode to hide an otherwise unnecessary editor footer, so that the document has more reading space.
24. As an author, I want source mode to retain useful editor status and word-wrap controls, so that editing information stays available.
25. As a terminal user, I want file tabs scoped to their terminal session and working in both docked and fill layouts, so that each session keeps its own documents.
26. As a remote workspace user, I want host identity included in duplicate detection and file operations, so that identical paths on different machines remain distinct.
27. As a reviewer, I want actual native Before/After screenshots embedded in the PR, so that I can assess the visual changes directly.

## Implementation Decisions

- Reuse the existing per-terminal collection of open files, source buffers, native Markdown views, file watchers and close/save protections. Do not add dependencies or a second document state system.
- Append new files in opening order and activate existing files in place. Identify files by owning host and canonical path; use stable buffer identity for UI actions that may complete after selection changes.
- Render a horizontal file tab strip above a dedicated breadcrumb and mode row. Paths never become tab labels, including for duplicate basenames. Breadcrumbs communicate location; directory navigation is not part of this feature.
- Keep document layout actions and the panel dismissal action distinct from each file's close control, including when the window title bar hosts document controls.
- Display a selected text segment for the current mode. Selecting the already active segment is idempotent. First-open defaults do not override a file's later mode choice; explicit line navigation still opens source.
- Remove the duplicate bottom preview toggle. Hide the editor footer while reading; retain source status and wrap controls. Keep HTML refresh and external-browser actions accessible next to the HTML mode controls.
- Preserve Markdown and source state using their existing persistent entities. Preserve ordinary HTML scroll with asynchronous browser evaluation and restoration associated with the owning file; ignore stale callbacks. Preserve the existing lazy single-browser lifecycle rather than retaining a browser process per file. Dynamic HTML DOM/form/JavaScript state is not promised across switches.
- Preserve existing HTML availability, resource loading and dirty-buffer save requirements. Linux and remote HTML retain existing source fallback behavior.
- When the last file closes, hide the code panel; an independently open diff view follows the existing document-front selection rules.
- Use existing theme tokens, file icons, localization and accessible button conventions. Handle long paths and narrow columns without overflowing mode controls or window chrome.
- Use the application theme's opaque base for source content, its gutter, the active file tab and breadcrumbs. Use the window hover surface for inactive tabs, the empty tab rail and header actions. Start tabs flush with the document edge, reserving space only for macOS traffic lights when they share the header. Keep syntax palettes selected by the application's light/dark appearance.
- Use the approved compact layout: a shared 32px window/document top strip with 28px chrome buttons, a 28px breadcrumb row and 24px panel-tab buttons. File-tab labels use 13/16 rem (13px at the default UI size), including the text measurement used to allocate tab widths. Keep sidebar/window backgrounds and transparent title-strip borders; align their heights and centers without adding horizontal separator lines. On Windows/Linux, the Files title row shares the breadcrumb row's 28px height.

## Testing Decisions

- User-confirmed seam: reuse the existing GPUI application-window harness and production document opening/actions. Drive actual tab, close and mode controls with pointer/keyboard events and inspect observable selection, document content, modes and protected edits. Prefer this existing high-level seam to new helper-unit tests or mocked renderers.
- Follow vertical red/green slices for stable multi-file navigation/closing, the two-row document controls, and view-state preservation. Reuse existing Markdown undo/save/focus, host ownership and native HTML regressions.
- Test same-basename files, tab overflow and reveal, inactive versus active close, final close, dirty close cancellation/save/discard, independent terminal sessions, and dock/fill/narrow layouts.
- Use the real native application for HTML scroll restoration and preview visibility; GPUI GPU captures alone cannot establish native child-WebView visibility.
- Capture a reproducible baseline at the integration branch's original commit and an after view with the same documents, theme and viewport. Include native screenshots for preview, source and mixed file tabs in the PR through repository-hosted image links.
- Run formatting, focused document tests, the workspace test suite and the host-boundary check. Report platform-specific checks that could not run locally rather than claiming they passed.

## Out of Scope

- Split editor groups, drag-reordering, pinned or temporary tabs, recently closed tabs, cross-session global file tabs and reopening files after application restart.
- A new Markdown or HTML renderer, new HTML platform support, arbitrary web browsing or retaining full live HTML page state between documents.
- Changing diff-review interaction or incorporating unrelated work already present in the original checkout.

## Further Notes

The user's Cursor screenshots define the layout: filename-only tabs, a separate path row, and Preview/Markdown text segments on that row's right. The bottom preview icon is removed. Publish the spec and implementation tickets in cloudy-liu/ctty7 with ready-for-agent; keep the spec issue as the parent and close implementation tickets through the fork PR. The PR must include visual Before/After evidence, not only test logs or design mockups.

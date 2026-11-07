# Preserved local changes for v0.7.1

Checked on 2026-10-21 against fork main at v0.7.0 (`f8adef01`), the open PR
list, local stashes, ctty7-related temporary files, and the local release backup.
No open pull requests existed at the start of the audit.

## Changes recovered for this release

| Preserved source | Finding | Disposition |
| --- | --- | --- |
| Stash `93f5a1ea`, created before v0.7.0 release preparation | Default HTML preview, lazy creation on first render, failure recovery, and regression coverage were not committed. | Recover the three tracked files, then check preview/source transitions and render-loop scheduling. |
| Stash `846600ab`, terminal rendering and theme validation sections | Explicitly colored Unicode block elements were still corrected as text, producing bright edges around TUI input fills. | Recover `src/terminal/element.rs` and the corresponding theme validation documentation and tests. |

The default-preview recovery patch applies cleanly to v0.7.0. The installed
v0.7.0 Windows executable matches the executable in the public portable package
byte for byte; opening the reported HTML from the file tree reproduces source
mode. This rules out an installed-package mismatch as the cause of that omission.

## Existing or archived work

| Preserved source | Evidence | Disposition |
| --- | --- | --- |
| Temporary `ctty7-12ec.patch` and `ctty7-5c45.patch` | Current `GitStatusCache` has `fail_probe`, `retry_due`, and unknown/negative-result recovery tests; terminal polling uses the retry policy. PR #171 includes the finalized behavior. | Already covered by later implementation; do not reapply old patch context. |
| Temporary document-header reproduction directories | Contain isolated configuration, daemon state, and scrollback, with no source checkout or patch. Document paths were merged in PR #174. | Validation artifacts, not pending product changes. |
| Stash `2bbc44c5` | Every tracked patch reverses cleanly from current main; the full-file diff work was merged in PR #158. | Already merged. |
| Remaining sections of stash `846600ab` | Cursor nested-pane detection is in PR #152 and the later manifest engine; diff selection and rendering have since moved into the implementations from PR #150, #154, and #158. | Superseded source context. |
| Stash `74055c25` | Contains the icon design lab associated with closed, unmerged PR #47. | Intentionally archived design work. |
| Stash `c73bcbb4` | Current source includes foreground host-application detection, daemon replay, shared OSC decoding, and sidebar naming; subsequent release and UI changes supersede the old context. | Existing behavior, not a new release feature. |
| v0.7.0 local-history bundles and release-test backup | Preserve integration history, fixtures, measurements, and test packages. Accepted PRs #165, #166, and #169 through #174 are present in main. | Historical recovery and validation artifacts. |

Original stashes and backup files remain available. This audit does not treat
commit ancestry alone as evidence that a squash-merged change is absent.

## Remaining product scope

Issue [#175](https://github.com/cloudy-liu/ctty7/issues/175) also covers macOS
WKWebView integration, platform boundaries, and further performance measurements.
The Windows default-opening correction completes only that portion of the spec;
the broader issue remains open.

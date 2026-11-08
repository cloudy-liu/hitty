# Preserved local changes for v0.8.0

Checked on 2026-10-23 against published v0.7.1, fork main, all open fork PRs,
local branches and worktrees, stashes, and ctty7-related temporary patches.

| Preserved source | Finding and disposition |
| --- | --- |
| `fix/file-tree-watch-registration` at `5ad7583d` | A committed but unpublished file-tree cache fix was recovered into PR #190. Local file-tree regression tests pass. |
| Uncommitted changes in `ctty7-tabs-html` | HTML scroll restoration and native regression coverage were preserved as a binary patch and committed into PR #191. The recreated hidden browser needed layout before scroll restoration; the same native acceptance failed before that correction and passed after it. |
| `feat/file-document-tabs`, `feat/file-tabs-ui`, `feat/file-tabs-html-scroll`, and `test/file-tabs-native-evidence` | Accepted integration and fixture work are included in squash-merged PR #186. Original feature branches retain the evidence ancestry. |
| `test/file-tabs-baseline` at `d099cd15` | Its native fixture registration patch has the same stable patch ID as integrated commit `8de7b3f5`; it adds no separate release behavior. |
| `fix/codex-hook-pane-routing`, `fix/one-light-markdown-code-background`, and `feat/diff-context-in-place` | Changes are included through squash merges #180, #181, and #187. |
| `fix/taskbar-attention-corner` | PR #189 contains the taskbar position correction. Its original Linux job passed build/tests but timed out while installing native screenshot dependencies; the job was rerun. |
| Existing five stashes and `ctty7-12ec.patch` / `ctty7-5c45.patch` | Their existing, superseded, or intentionally archived dispositions remain as recorded in the v0.7.1 audit. Originals are retained. |

Before cleanup, the original branch and stash history was saved in
`D:/gh-prj/ctty7-release-backup-20261023-v080/local-history.bundle`, alongside
`html-scroll-original.patch`. The backup also retains the native acceptance
logs, including the failing and passing scroll-restoration runs. Worktrees are
removed only after their changes are included or their recovery is established.

The release range also includes already-merged #177, #178, and #179, which
postdate v0.7.1. No patch-equivalent history synchronization is presented as a
new product capability.

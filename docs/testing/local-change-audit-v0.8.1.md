# Preserved local changes for v0.8.1

Checked on 2026-10-24 against the latest published release v0.8.0, fork main
at `643d3c18`, the complete release range, local worktrees, all six stashes,
ctty7-related temporary patches, and the closeout recovery inventory.
No open fork pull requests or issues remained before release preparation.

| Preserved source | Finding and disposition |
| --- | --- |
| `fix/html-preview-fragment-state`, `fix/taskbar-clear-attention-icon`, and `fix/tray-attention-badge-size` | Accepted behavior is included through squash-merged PRs #193, #194, and #196. The hidden-tray visual limitation was explicitly accepted and remains unverified. |
| `feat/titlebar-direct-actions` and `fix/compact-document-chrome` | The final accepted menu scope is two icons in the existing menu, merged through #200; compact headers and aligned chrome are merged through #201. The old branch name does not imply new titlebar actions. |
| `fix/mermaid-reading-and-mouse-zoom` at `0194cdac` | The final tested implementation is squash-merged through #197. Its integrated Windows workspace run passed 3,162 tests with 12 ignored and no failures; all ten final PR checks passed. Missing final native Windows screenshots were explicitly accepted and remain unverified. |
| `chore/pr-closeout-20261024` at `93e41734` and `docs/readme-current-screenshots` | Final native documentation captures, fixture work, and provenance are integrated through #202. All ten final PR checks passed. Captures use the recorded v0.8.0 renderer baseline rather than claiming to show the later compact headers. |
| Stash `9c1dc9ea` | Its earlier README and screenshot-link edits are superseded by #202. The untracked `run-local-release.cmd` remains a local launcher, preserved both in the stash and recovery archive; it is not product scope. |
| Stashes `93f5a1ea`, `2bbc44c5`, `846600ab`, `74055c25`, and `c73bcbb4` | Their included, superseded, or intentionally archived dispositions remain as recorded in the [v0.7.1 audit](local-change-audit-v0.7.1.md) and [v0.8.0 audit](local-change-audit-v0.8.0.md). All originals remain available. |
| Temporary `ctty7-12ec.patch` and `ctty7-5c45.patch` | These are the previously audited Git-status recovery patches covered by #171; they remain preserved and add no new release behavior. |

Before closeout cleanup, original branch tips and all stashes were saved in
`D:/gh-prj/ctty7/.git/archives/closeout-20261024/local-history.bundle`, with
`branches.txt`, `stashes.txt`, the local launcher, and integrated test logs.
The bundle was verified before auxiliary worktrees and temporary branches
were removed. Release preparation starts with only the main worktree and
its clean main branch; the new release branch contains only release metadata
and documentation changes.

The full range from v0.8.0 includes #193, #194, #196, #197, #200, #201, and
#202. No history-only synchronization is presented as new product behavior.
Published v0.8.0 and all earlier tags and assets remain unchanged.

# Preserved local changes for v0.8.3

Checked on 2026-10-28 against the latest published release v0.8.2, the complete
candidate range, all nine registered worktrees and all six historical stashes.
Release preparation changes version metadata and documentation. Product changes
come from accepted fork PRs; #214 completes CI before its merge.

| Preserved source | Finding and disposition |
| --- | --- |
| `release/v0.8.3` worktree | The earlier release commit `2d4a610c` is retained. The candidate also integrates fork main through #213 and the exact #214 source, updates the English notes, and corrects stale configuration and component documentation. |
| `feat/git-history-avatars` at `eb046777` | Author photos, compact rows and independent commit documents are included through cloudy-liu/ctty7#209. Original source and acceptance records are retained in Git history. |
| `fix/editor-horizontal-scrollbar` at `41895513` | Source overflow scrollbars and the component policy are included through cloudy-liu/ctty7#210 at `98aa012e`. |
| `fix/unsupported-preview-hints` | Unsupported-preview behavior is included through cloudy-liu/ctty7#211. Its later folder-rename correction at `b8dd3a5b` and native screenshots are tracked by cloudy-liu/ctty7#214. Uncommitted Mermaid-control simplification, prototypes and custom-name hover changes are independent unfinished work; they are excluded and preserved before local synchronization. |
| `fix/background-agent-attention` at `ec4046ab` | The installed Windows taskbar correction is included through cloudy-liu/ctty7#212 at `d13a2ae9`; it is no longer unfinished work. |
| `fix/diff-file-name-copy` at `be5f12e1` | Diff file-name selection is included through cloudy-liu/ctty7#213 at `46e609d6`. |
| `ctty7-preview-checkbox` worktree | Earlier preview implementation already included through #211; its duplicate build output is disposable after archival. |
| `ctty7-before` worktree and sibling target | Used only to build exact #214 Before/After revisions. Durable captures are committed under `docs/testing/images/file-tree-rename-selection/`; the worktree and binaries are disposable after acceptance. |
| `.scratch/history-current-baseline` worktree | The modified `src/document_visual.rs` is an opt-in native measurement fixture. Preserve its patch before removing the temporary worktree; it is excluded from production source. |
| Historical stashes `9c1dc9ea`, `93f5a1ea`, `2bbc44c5`, `846600ab`, `74055c25`, `c73bcbb4` | Hashes, file lists and descriptions were checked again. Included, superseded and intentionally archived dispositions remain as recorded in the [v0.8.2 audit](local-change-audit-v0.8.2.md) and its references; no historical stash is applied or dropped. |
| Closeout stashes `441de1cc` and `cf31cd21` | Preserve the unfinished Mermaid/sidebar changes (including untracked prototypes and screenshots) and the native History measurement fixture, respectively. Both original worktrees are clean after preservation. |

The complete v0.8.2 range includes #206 and #209–#214. Inherited capabilities
and patch-equivalent integration history are not counted as new behavior.
The earlier History CI artifact timeout and intermittent Markdown-selection
failure remain in [History acceptance](git-history-avatars.md) and the original
[successful source CI attempt](https://github.com/cloudy-liu/ctty7/actions/runs/40452423904/attempts/3).

Branch tips, worktree positions and all stash/reflog history are archived in a
verified Git bundle under `.git/archives/closeout-20261028/`, alongside ref,
stash and worktree manifests. Refresh it before final branch cleanup.
Unfinished local work is
archival material, not a release capability. Rebuildable target directories and
isolated native-test configurations can then be cleaned. Historical published
tags, releases, assets and user-installed application data remain intact.

See [English release notes](../releases/v0.8.3.md), [folder-rename acceptance](file-tree-rename-selection.md),
and [issue audit](open-issue-audit-2026-10-27.md). Require final candidate source
CI, all platform package jobs, every downloaded release asset and checksums
before explicitly publishing the draft release.

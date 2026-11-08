# Preserved local changes for v0.9.0

Checked on 2026-10-29 against the latest published release v0.8.3, the complete
candidate range, registered worktrees, and all eight retained stashes.

## 2026-10-31 preparation update

The published baseline was still v0.8.3. Fork main had gained sidebar group
icons (#227), the deleted-document close fix (#228), and grouped-row indentation
(#229) after the initial release preparation. SSH host groups (#230) and the
Scratch Inbox icon (#233) are dependencies of the updated release notes and
must be merged and verified before the release is tagged.

The primary checkout's untracked `docs/prototypes/` and `docs/research/` remain
outside the release. The separate settings-surface and background-attention
branches contain independent work and are preserved. The integration worktree
`D:/gh-prj/ctty7-release-0.9.0` still has uncommitted documentation changes;
they were not cleared during this audit. All eight stash hashes below were
rechecked and remain unchanged. Temporary fixture worktrees are verification
material, not additional product scope.

## Initial preparation audit

| Preserved source | Finding and disposition |
| --- | --- |
| Native SSH Git at `70bd558b` | Included through cloudy-liu/ctty7#224: pane-owned read-only transport, status/diff routing, bounded refresh, lifetime cleanup, and native captures. Integration with SSH restoration also registers a Git host when reattaching a pane. |
| SSH restoration at `0d47fc94` | Included through cloudy-liu/ctty7#217: exact supported-agent resumes, remote prompt readiness, live reattach, missing-pane redial, and native fixture evidence. |
| Commit layout at `ac0221ca` | Included through cloudy-liu/ctty7#218. Final native toolbar, divider, and ordinary-file captures resolve the earlier draft evidence blocker. |
| Working comet at `78173afe` | Included through cloudy-liu/ctty7#219, with native light/dark evidence and visible-only animation checks. |
| `integration/v0.9.0-check` | Combines the four product branches before squash merge and verifies their interaction. Its local merge history is validation material, not a separate feature. |
| Diff background at `a223138c` | The concurrent local Diff-background correction became cloudy-liu/ctty7#225 and is included. Additional matched LCU captures verify the running Before/After applications. |
| Original `D:/gh-prj/ctty7` checkout | Left untouched. Untracked `docs/prototypes/` remain research artifacts rather than extra production changes. |
| `fix/background-attention-on-return` at `57a2e4a5` | Separate local work without an open PR in the release inventory; excluded and preserved for its own review and acceptance. |
| Detached upstream checkout at `9dd06c63` | Clean upstream reference, retained unchanged; not a new fork capability. |
| `target/ssh-resume-before` at `cb0bc217` | Clean baseline used for restoration evidence, retained with its isolated fixture. |
| Five Native Git implementation worktrees | Removed only after verifying each was clean and its tip was an ancestor of the integration branch. Their branches and committed evidence remain in Git. Other worktrees were not removed. |

Stashes `cf31cd21` and `441de1cc` retain native History measurement and unfinished
Mermaid/sidebar work as recorded in the [v0.8.3 audit](local-change-audit-v0.8.3.md).
Stashes `9c1dc9ea`, `93f5a1ea`, `2bbc44c5`, `846600ab`, `74055c25`, and `c73bcbb4`
match the previously audited hashes and descriptions; their included, superseded,
or intentionally archived dispositions remain in the [v0.8.2 audit](local-change-audit-v0.8.2.md)
and its references. No stash is applied or dropped.

The full v0.8.3 range also includes the macOS 15 source-CI runner change in
cloudy-liu/ctty7#216. Release preparation updates shared version metadata,
English notes, SSH/diff usage, and the dated project status. Inherited
capabilities and local integration merges are not presented as new features.

The first Intel macOS check for #224 hit the previously recorded
`fill_and_restore_preserve_a_partial_markdown_selection` intermittent failure
before selecting its initial word. Its failed log is retained outside the
repository. The combined workspace and 30 consecutive Windows repetitions pass;
require the final native CI verdict before merging.

Keep the release a draft until final source CI, all platform package jobs,
all twelve downloaded program assets, and `checksums.txt` pass verification.
See the [release runbook](../maintenance/release.md) and
[English notes](../releases/v0.9.0.md).

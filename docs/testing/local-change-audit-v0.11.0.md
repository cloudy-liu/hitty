# Local-change audit for v0.11.0

Checked on 2026-11-05 against the fork's published v0.10.0, the complete first-parent release range, all registered worktrees, and seven retained stashes.

## Release range

The fork's v0.10.0 annotated tag resolves to the release preparation merge e423703a. The ordinary local v0.10.0 tag belongs to upstream history, so the fork baseline was fetched explicitly as refs/tags/ctty7/v0.10.0. Release preparation uses the fork baseline, without moving either existing tag.

The complete product range contains cloudy-liu/ctty7#244 (attention sizes), #250 (file selection and recoverable deletion), #252 (document tabs), and #253 (Qoder/CodeBuddy). All three open PRs were squash-merged on explicit maintainer instruction; #250 was converted from draft first. Its stale early-slice description was replaced with final implementation evidence and the retained screenshot limitation. The release preparation changes only package versions and release documentation.

## Preserved local work

| Source | Finding and disposition |
| --- | --- |
| Root feat/attention-badge-size checkout at 0e1bbe15 | Its product changes were already squash-merged as #244; leave the checkout and its untracked designs, research, spec copy, and test log intact. |
| feat/file-selection-trash, feat/editor-tab-curves, feat/qoder-codebuddy-agents | Product changes are included through #250, #252, and #253; leave their checkouts and evidence intact. |
| feat/ticket-246-trash, feat/ticket-247-multiselect, feat/ticket-248-remote-trash, feat/ticket-249-sftp-trash | Constituent ticket implementations are included through #250; preserve these branches and checkouts. |
| research/herdr-core-integration at e423703a | Three modified files in core/config.rs, daemon/mod.rs, and daemon/pane.rs belong to the independent #251 spike; preserve them outside this release. |
| fix/background-attention-on-return at 57a2e4a5 | Independent preserved work; no new changes imported into this release. |
| feat/settings-translucent-surface at 42863db4 | Independent settings-surface work remains outside this release. |
| Detached qoder-before at 44b2e11a | Baseline evidence checkout; preserve it. |
| Stashes 441de1cc, 9c1dc9ea, 93f5a1ea, 2bbc44c5, 846600ab, 74055c25, c73bcbb4 | All seven are retained with their previous audit dispositions; none was applied or dropped. |

Temporary ctty7 verification directories were inventoried. They contain capture, prototype, native-check, and prior-release verification material; none is imported as product source. No ctty7/tty7 .patch or .diff file was found at the temporary-directory top level. Source work and build outputs are preserved; this release does not perform cleanup.

## Acceptance and publication

The maintainer explicitly requested merging the draft and publishing the next release. The #250 matching Before screenshot and the #244 durable capture limitations remain disclosed as unverified, rather than described as completed acceptance. See the [English notes](../releases/v0.11.0.md) and the existing feature verification records.

Require successful final source CI and all Release jobs. Download the assembled draft's complete twelve program assets plus checksums.txt into a fresh directory and run the existing checksum verifier before explicit publication and Latest selection. Keep manual platform and installation limits in the release notes.

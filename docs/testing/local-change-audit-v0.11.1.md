# Local-change audit for v0.11.1

Checked on 2026-11-06 against the published fork v0.11.0 tag, the complete first-parent release range, registered worktrees, the root working-tree diff, and seven retained stashes.

## Release range

The fork baseline is refs/tags/ctty7/v0.11.0. The accepted range contains cloudy-liu/ctty7#255, #257, and #258. PR #258 was converted from draft and squash-merged on explicit maintainer instruction after all ten head checks passed. Its merge-tree delta against main changes only pane chrome; its attention-size history is patch-equivalent to #244 and adds no new behavior.

## Preserved local work

The root checkout remains on feat/attention-badge-size. Its two modified SSH fixture/sidebar files implement the settings behavior and regression coverage already included by #255; preserve them along with all untracked prototypes, research, specs, and logs. The #257 worktree is included through its squash merge. Other registered worktrees and all seven stashes retain the dispositions recorded in the v0.11.0 audit and remain untouched. Independent Herdr-core, settings-surface, and background-attention work is outside this release.

## Verification and publication

The final #257 head passed all ten CI jobs after the Markdown heading-selection fixture was made independent of platform font height. PR #258 also passed all ten head checks. Require successful candidate CI and Release jobs, then download all twelve program assets and checksums.txt and run the complete asset verifier before explicitly publishing Latest. The release notes retain live-session, visual acceptance, package signing, and interactive installation limitations.

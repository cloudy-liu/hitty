# Local-change audit for v0.10.0

Checked on 2026-11-02 against published v0.9.0, the candidate range, registered worktrees, local branches, and all seven retained stashes.

## Release range

Fork main now contains PRs #236 through #241. PRs #239 and #241 were squash-merged after all fork CI checks passed. The release notes describe only the user-visible changes in the v0.9.0-to-v0.10.0 range. The release preparation commit changes shared package versions and release documentation; it does not add product behavior.

## Preserved local work

| Source | Finding and disposition |
| --- | --- |
| D:/gh-prj/ctty7 untracked .impeccable/, docs/prototypes/, and docs/research/ | Preserve as local design and research material outside the release. The taskbar-attention prototype overlaps PR #241 but remains a local untracked copy until the fork main sync; remove only that exact file after confirming the merged tree contains identical bytes. |
| fix/document-tabs-radius at d427401e, worktree D:/gh-prj/ctty7-agent-pane-titles | Preserve the unique document-tab radius commit and local branch; remove its clean checkout after publication to release the worktree directory. It is not part of v0.10.0. |
| fix/background-attention-on-return at 57a2e4a5, worktree D:/gh-prj/ctty7-background-attention | Preserve the separate branch and its worktree; it is outside the accepted release scope. |
| feat/settings-translucent-surface at 42863db4, worktree D:/gh-prj/ctty7-settings-surface | Preserve its unique commit and uncommitted source changes. Cargo build output may be cleaned after publication without changing source. |
| Stashes 441de1cc, 9c1dc9ea, 93f5a1ea, 2bbc44c5, 846600ab, 74055c25, and c73bcbb4 | All seven hashes remain in the stash list. None was applied or dropped; prior release audits retain their individual dispositions. The unfinished Mermaid/sidebar experiment remains archived even though #237 ships a toolbar revision. |

## Temporary verification material

| Source | Finding and disposition |
| --- | --- |
| target/attention-return-baseline-src | Clean detached baseline nested under the root Cargo target. Remove the registered worktree before running cargo clean on the root. |
| D:/gh-prj/ctty7-agent-pane-before | Contains an uncommitted visual fixture module and one source handler matching the PR #239 implementation. Remove only after #239 is merged and both files are rechecked against fork main. |
| D:/gh-prj/ctty7-attention-dot | Clean checkout of PR #241. Remove the worktree and local branch after the squash merge is confirmed. |
| D:/gh-prj/ctty7-attention-return-v090 | Verification/evidence checkout for merged PR #238. Remove after confirming its evidence is present in fork main. |
| D:/gh-prj/ctty7-claude-evidence-src | Detached source checkout whose three modified files currently match fork main byte-for-byte. Remove after rechecking the merged tree and confirming there are no other local or untracked files. |
| Root target, settings target, and .git/native-macos-check/target | Build outputs measured at approximately 30.7 GB, 9.8 GB, and 250 MB. Run cargo clean in those workspaces only after release publication; preserve the standalone native-macos-check source harness. |
| fix/claude-input-detection at dddf2128 | Root checkout of the merged PR #240 head. Keep the root's untracked research directories; after publication, switch the root to main, fast-forward it to fork/main, and remove this merged local branch. |
| tune-sidebar-tab-font-size at 9cb37686 | Exact local copy of the merged PR #236 head; remove this local branch after verifying the fork main contains the merged change. Do not delete the remote branch. |
| feat/agent-pane-titles-234 at 6410fb07 | Exact local copy of merged PR #239; remove this local branch after verifying its squash merge. |
| fix/attention-dot at f8bd59ea | Exact local copy of merged PR #241; remove this local branch after verifying its squash merge. |
| fix/background-attention-return-v090 at 873a4478 | Remove this local branch only after confirming its merged PR #238 evidence and changes are present in fork main. |
| release/v0.10.0 | Remove the release preparation checkout and local branch after the release PR is squash-merged; its commit remains in main history. |

No remote branch is deleted by this audit. Remove only verified temporary local worktrees and branches after publication. Keep the release a draft until all twelve program assets and checksums.txt pass the existing downloaded-asset verifier.

## Release limits

The release notes retain the platform-specific visual acceptance and package-installation limits. The installed Windows taskbar-group clearing result from PR #238 remains inconclusive. No retained stash or independent local branch is included as new release behavior.

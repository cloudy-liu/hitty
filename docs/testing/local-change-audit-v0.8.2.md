# Preserved local changes for v0.8.2

Checked on 2026-10-25 against the latest published release v0.8.1, fork main
at `4020a1cc`, the complete release range, the local worktree and branches, all
six preserved stashes, and the ctty7-related temporary patches.

| Preserved source | Finding and disposition |
| --- | --- |
| `fix/compact-preview-native-tooltips` at `48f5eed6` | Patch-equivalent to merged cloudy-liu/ctty7#204 at `4020a1cc`; included in this release. The branch and its original commit remain preserved. |
| Current direct-titlebar and flat-preview changes at `7bc97356` | Included through release PR cloudy-liu/ctty7#205 with matched native Windows screenshots and rendered interaction coverage. The shared 28px titlebar geometry and 24px preview target are retained. |
| Historical remote `feat/titlebar-direct-actions` | Earlier implementation and menu-only revision remain preserved. The current release branch restores the later requested direct-entry behavior without rewriting that historical branch. |
| Six stashes from `9c1dc9ea` through `c73bcbb4` | Hashes and descriptions match the previous release audit. Their included, superseded, or intentionally archived dispositions remain as recorded in the [v0.8.1 audit](local-change-audit-v0.8.1.md); none were dropped or applied. |
| Temporary `ctty7-12ec.patch` and `ctty7-5c45.patch` | The previously audited Git-status recovery patches remain preserved and introduce no additional release behavior. |
| `target/tmp/titlebar-direct-actions` | Contains local validation binaries, fixture options, and PR/commit text. Durable acceptance screenshots are committed under `docs/testing/images/titlebar-direct-actions`; local binaries and scratch configuration are excluded from the release source. |

Only the main checkout is registered as a worktree. The earlier closeout bundle
under `.git/archives/closeout-20261024` remains preserved. No stashes, historical
tags, or published assets are removed or rewritten during this release.

The Linux native capture script labels the trailing action as Settings for the
current titlebar and retains menu capture names for older baselines, so the
comparison artifacts describe the interface they actually capture.

The complete range from v0.8.1 includes the document-preview and native-tooltip
changes in #204, the direct titlebar and transparent preview control in #205,
and release version/documentation updates. Patch-equivalent source already
merged in #204 is counted once. No history-only synchronization is presented
as new product behavior.

See [the English release notes](../releases/v0.8.2.md) and
[titlebar acceptance](titlebar-direct-actions.md) for the delivered behavior and
remaining verification limits. Source CI, package verification, downloaded
assets, and checksums must pass before the draft release is published.

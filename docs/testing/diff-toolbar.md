# Fixed Diff toolbar and bulk folding

Spec: #52; implementation: #53 and #54. Baseline:
`c552bbec607f3243a29f4c5c0f4ef8e0713f1bee`.
Final implementation: `7a7ef8614dbc123d5379f219c3513c003fa95256`.
Native acceptance: Windows, November 20, 2026.

## Reproduce native captures

Build the baseline and implementation in separate worktrees, copying each
binary and its adjacent runtime DLLs before the next build:

```powershell
cargo build --locked --features markdown-visual-tests --bin hitty-app
python scripts/prepare-diff-toolbar-captures.py D:/tmp/diff-toolbar-fixture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture-diff-toolbar.ps1 `
  -Binary <baseline-or-implementation>/hitty-app.exe `
  -FixtureDirectory D:/tmp/diff-toolbar-fixture `
  -ConfigDirectory D:/tmp/diff-toolbar-<phase>-<theme>-config `
  -EvidenceDirectory D:/tmp/diff-toolbar-evidence `
  -Theme <light-or-dark> -Phase <before-or-after>
```

Use a new configuration directory for each invocation. The capture driver
opens Git, selects the latest commit, captures its document, then opens the
session's worktree diff. It uses the existing PrintWindow capture script and
posts input to only the fixture window. The actual pointer never moves.

Fixture: eight changed Rust files, commit
`a8c21ec808b5861ab46d0f1c2e5468b0dbabd03b`, +16/-16 in the commit and +8/-8 in
the worktree. Both themes use a 1280 x 800 logical viewport, 300 px right
panel and 550 px History section. Captures on Windows use 175% display scale:
2240 x 1400 client, 2244 x 1402 saved frame. The script reads the fixture's
reported scale before posting input.

## Evidence

Native screenshots are under [images/diff-toolbar](images/diff-toolbar).
Before and After use the same fixture, viewport, display scale, theme and
input sequence. No browser rendering stands in for the application.

| Scene | Before | After |
| --- | --- | --- |
| Commit, light | [Image](images/diff-toolbar/before-commit-light.png) | [Image](images/diff-toolbar/after-commit-light.png) |
| Commit, dark | [Image](images/diff-toolbar/before-commit-dark.png) | [Image](images/diff-toolbar/after-commit-dark.png) |
| Worktree, light | [Image](images/diff-toolbar/before-worktree-light.png) | [Image](images/diff-toolbar/after-worktree-light.png) |
| Worktree, dark | [Image](images/diff-toolbar/before-worktree-dark.png) | [Image](images/diff-toolbar/after-worktree-dark.png) |

Inspected all eight matching images. The commit strip holds the existing totals
and icon-only controls below the divider; the worktree title bar adds the bulk
button beside layout. Both use the original double chevrons for file folding.

Additional native checks used the same isolated fixture and posted input:

- Scrolled the commit list eight wheel notches, then collapsed and expanded
  all cards. The strip stayed fixed and `diff_context.rs`, the partially
  visible reading file, remained visible after both actions:
  [scrolled](images/diff-toolbar/after-commit-scrolled-light.png),
  [collapsed](images/diff-toolbar/after-commit-collapsed-light.png),
  [expanded again](images/diff-toolbar/after-commit-restored-light.png).
- Folded all eight worktree files, expanded them, scrolled twelve wheel
  notches and repeated the toggle. The reading file remained visible even
  though the collapsed list fit without scrolling:
  [folded worktree](images/diff-toolbar/after-worktree-collapsed-light.png).
- Switched the commit diff to unified layout, opened one worktree file through
  Changes, then expanded its full context. Bulk folding was hidden while
  layout, context and change navigation remained available:
  [focused full context](images/diff-toolbar/after-focused-context-light.png).

For supplemental captures, use the PID printed by the driver with
`scripts/post-mouse-wheel.ps1` and `scripts/capture-native-window.ps1`.
Coordinates passed to those scripts are physical pixels; multiply logical
coordinates by the scale reported in `<phase>-<theme>-ready.json`. Move the
synthetic pointer to `1,1` before saving each image.

## Validation

The GPUI tests click rendered controls and observe card visibility, scroll
position and control bounds. Coverage includes mixed fold states, automatic
fold defaults, explicit expansion of large files, binary/empty/loading and
truncated diffs, the rendered-file limit, keyboard activation, both layouts,
commit resizing, and focused controls at 560 and 360 logical pixels.

The two-axis review found a stale reading anchor after switching between split
and unified layouts. A replacement-heavy regression first reproduced the
failure, then passed for both worktree and commit controls after invalidating
the remembered reader when the rendered layout changes. Existing scroll-clamp
round trips still pass. Shared test fixtures were also extracted. Both the
specification and standards reviewers verified their findings were resolved.

Passed on the implementation:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets --features updater
cargo build --locked --features markdown-visual-tests --bin hitty-app
python scripts/sync-agent-detection.py --check
python -m unittest discover -s scripts -p 'test_sync_agent_detection.py'
bash .github/scripts/check-host-boundary.sh
```

Python: 15 manifests verified, 4 tests passed. Host boundary: 118 UI/terminal
files checked. Existing compiler warnings remain; dependency pins are unchanged.

The full `cargo test --locked --workspace --features updater --
--test-threads=1` run completed the desktop, updater, CLI and core groups, then
stopped on the core failure below. The remaining server targets and doctests
were run separately with `cargo test --locked -p hitty-server --
--test-threads=1` and `cargo test --locked --workspace --features updater --doc
-- --test-threads=1`.

| Group | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Desktop application | 1,882 | 0 | 7 |
| Updater | 32 | 0 | 1 |
| CLI | 138 | 0 | 0 |
| Core library | 1,268 | 1 | 5 |
| Server and integration tests | 30 | 0 | 1 |
| Doctests | 0 | 0 | 0 |

The failing test is
`daemon::pane::tests::windows_falls_back_to_the_process_tree_when_the_shell_reports_no_c`.
It failed in both the initial four-thread run and the final serial run, while
an exact-filter retry of the final workspace test binary passed (1 passed).
The core crate, workspace manifest and lockfile are unchanged from the baseline;
the complete core tree has Git object ID
`1b2d1f876f291a19734fe2ba7fa4295a90888fe2` in both revisions.

A clean detached checkout of baseline `c552bbe` passed the complete core group
with `cargo test --locked -p hitty-core --lib --features gssapi,remote-install
-- --test-threads=1`: 1,269 passed, 0 failed, 5 ignored. The final implementation
then passed the same complete core command with the same counts. All groups
therefore passed across the completed runs; the combined workspace invocation
above still records its original failure.

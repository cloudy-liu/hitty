# Hitty bootstrap verification

The source baseline is `4699e7e`, imported from source revision
`7f430a8f5f89a3a2963250b196b0baea02569299`. This record covers the rebranding
work; it does not claim that the first release is ready or published.

## Completed locally

- The baseline's 1,534 imported source files match the source revision's Git
  blobs and executable modes. The existing Hitty README and new project
  guidance/provenance are documented exceptions. The history still has only
  the Hitty bootstrap root, with no imported tags or source ancestry.
- The baseline Windows workspace build passed.
- `cargo build --locked --workspace --bins --features updater` passed for Hitty
  on Windows x86_64, including GUI, CLI, server, and updater.
- `cargo test --locked --workspace --features updater` passed on Windows:
  3,280 tests passed across the standard Rust test summaries, with 13 existing
  ignores. The separate CLI end-to-end harness also passed.
- `cargo metadata --locked --offline --no-deps --format-version 1` resolves all
  four workspace packages at version `0.1.0`.
- All 1,268 external lockfile entries retain their source, version, and checksum.
- The configuration default implementation, original app/tray icons, and
  original license bytes are unchanged.
- Active source, owned crates/skills/scripts, manifests, and build workflows
  have no old product-name occurrences. Historical records and attribution
  retain their truthful source names.
- `python scripts/sync-agent-detection.py --check` verified all 15 pinned
  manifests; its four existing Python tests passed.
- Formatting, changed-file whitespace, Bash/Python/PowerShell packaging syntax,
  wordmark XML, and documentation JSON checks passed.

## Native application evidence

The same smoke harness launched each debug application with a separate temporary
configuration, a 1100 by 700 logical-pixel window, and identical preferences.
Only test isolation settings were overridden: update checks/downloads, CLI PATH
installation, and the tray icon were disabled, and PowerShell used no profile.
The public CLI found the first pane, renamed its workspace, sent a command,
and read back `BOOTSTRAP_OK`. Both runs passed.

The baseline CLI reported `tty7 0.11.1`; the new CLI reported `hitty 0.1.0`.
PrintWindow captured the same 1929 by 1227 frame at 175% scale without a pointer.
Both saved images were inspected. Compare the unchanged application icon and
terminal layout, the changed default tab identity, and successful command output.

![Baseline application](images/hitty-bootstrap/before-application.png)

![Hitty application](images/hitty-bootstrap/after-application.png)

The capture commands, shell output, and isolated settings are recorded beside
the images. Reproduce the current application check with:

```powershell
python scripts/bootstrap-native-smoke.py target/debug/hitty-app.exe target/debug/hitty.exe docs/testing/images/hitty-bootstrap --phase after
```

## Final brand presentation

The README and documentation use the exact approved icon-plus-wordmark paths
in separate light and dark variants. The SVGs parsed successfully, the copies
used by the documentation match the source assets, and a local headless Chrome
render was inspected against the supplied reference. This is brand-asset
evidence; the native application screenshots above remain the application proof.

![Approved light and dark logos](images/hitty-bootstrap/final-brand.png)

## Cross-platform CI

CI run [42427069652](https://github.com/cloudy-liu/hitty/actions/runs/42427069652)
passed all 10 jobs at `cec12e0aa159d82e8d150765edc2d52c567024d0` on
November 8, 2026 (Asia/Shanghai):

| Target | Locked build, workspace tests, updater tests | Native fixtures |
| --- | --- | --- |
| Windows x86_64 | Passed | Local before/after application evidence above |
| Linux x86_64 | Passed | Markdown and SSH-tab fixture captures |
| macOS ARM64 | Passed | Markdown and HTML preview checks |
| macOS x86_64 | Passed | Markdown and HTML preview checks |

Both Linux musl server architectures, both macOS server architectures,
formatting, pinned Agent detection manifests, and the host boundary check also
passed. The earlier run `42401272370` exhausted the Intel build's 30-minute
limit while compiling. The current workflow allows 45 minutes for that build
and 90 for its job; no tests or visual steps were removed. In this completed
run the cached Intel build took 13m 36s, followed by successful tests and captures.

The two failures recorded on the source baseline both passed in the complete
Intel test job `128567512741`:

```text
ui::markdown_preview::tests::fill_and_restore_preserve_a_partial_markdown_selection ... ok
ui::scm::graph::tests::selected_commit_shows_message_details_above_the_existing_diff ... ok
```

This verifies their current result on Intel; it does not establish the cause
of the earlier failures or claim a production behavior fix for them.

Representative CI images were downloaded and inspected. The saved files retain
the original artifact bytes; `images/hitty-bootstrap/ci-cec12e0.json` records
their source paths and SHA-256 values. Markdown content, theme colors, and the
HTML page are visible. The ARM64 runner's small desktop clips the fixture
window, so those captures do not establish complete window-layout acceptance.
Both HTML result records passed resource loading, script interaction, refresh,
scroll ownership, tab switching, and navigation-failure recovery.

![Linux Markdown fixture](images/hitty-bootstrap/linux-markdown-light.png)

![macOS Intel Markdown fixture](images/hitty-bootstrap/macos-intel-markdown-dark.png)

![macOS Intel native HTML preview](images/hitty-bootstrap/macos-intel-html.png)

## Package acceptance

The release workflow was unavailable for manual branch dispatch because it is
not yet on the repository's default branch. It now also builds packaging-related
PR changes with read-only repository access and ad-hoc macOS signing. Branch
runs assemble and verify the complete 12-asset set and SHA-256 file as an Actions
artifact. Only a version tag can reach the existing draft-release job.

Package runtime checks exercise the extracted application and public CLI with
isolated user data, then restart the GUI and verify the existing pane still
accepts commands. The Windows check additionally installs into a temporary
location, checks its product registration, runs the installed app, and uninstalls
it on the disposable CI runner. The smoke harness extension passed locally with
the Windows debug build; final package results remain pending.

## Pending acceptance

- Run the new package workflow and inspect its runtime evidence, installer
  results, complete artifact set, and checksums. The macOS PR packages are
  ad-hoc signed and do not establish notarized distribution acceptance.
- Actual remote SSH/WSL deployment and updates between distinct packaged Hitty
  versions remain unverified. Existing automated updater tests are passing.
- No tag, public release, signing claim, or completed first-release gate is made.

## Follow-up regression checks

The rename initially exposed capitalization expectations and a color fixture
whose crate name changed byte length. Expectations now use the displayed Hitty
name; the captured color fixture uses an equal-width neutral name so all
original token offsets and colors remain valid.

The complete test run also exposed intermittent clicks on animated dialogs and
notifications. The file-tree subset reproduced the failure under parallel
execution; the individual test and serial subset passed. Adding a 100 ms gap
between reading a checkbox's bounds and clicking made the same assertion fail
deterministically. The component uses a 250 ms wall-clock entrance animation,
so advancing the test executor does not finish the movement. The affected tests
now complete the animation and redraw before measuring click targets. The delay
remains in the regression test, and 10 parallel subset runs (310 executions)
passed after the correction. Production animation behavior is unchanged.

Linux CI also found an inherited integration test that still required old-data
adoption, which is explicitly outside Hitty's scope. It now verifies that a
successful fresh server startup leaves data in other directories untouched,
for both default and explicitly selected profiles. That correction passed Linux
and macOS ARM64 CI. A local WSL offline attempt could not resolve the uncached
pinned framework checkout and did not execute the test.

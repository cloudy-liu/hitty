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

## Pending acceptance

- macOS ARM64, macOS x86_64, and Linux builds/tests and native runtime evidence
  require their respective runners.
- The selected source had two macOS Intel CI failures: Markdown partial
  selection after double-click, and the commit-detail back-to-all-files action.
  They remain unresolved until reproduced and fixed or verified with evidence;
  a Windows build does not establish their status.
- Final installer execution, remote SSH/WSL services, and updates between actual
  Hitty packages remain unverified. Inno Setup is unavailable on this host.
- No tag, public release, signing claim, or three-platform completion is made.

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
for both default and explicitly selected profiles. That Unix-only correction
awaits the next platform CI run. A local WSL offline attempt could not resolve
the uncached pinned framework checkout and did not execute the test.

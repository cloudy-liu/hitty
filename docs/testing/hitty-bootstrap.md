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

## Pending acceptance

- The full Windows workspace/updater test run is in progress.
- macOS ARM64, macOS x86_64, and Linux builds/tests and native runtime evidence
  require their respective runners.
- The selected source had two macOS Intel CI failures: Markdown partial
  selection after double-click, and the commit-detail back-to-all-files action.
  They remain unresolved until reproduced and fixed or verified with evidence;
  a Windows build does not establish their status.
- Final installer execution, remote SSH/WSL services, and updates between actual
  Hitty packages remain unverified. Inno Setup is unavailable on this host.
- No tag, public release, signing claim, or three-platform completion is made.

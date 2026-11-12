# Contributor validation

Run commands from the repository root with the locked dependency graph:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets --features updater
cargo test --locked --workspace --features updater
python scripts/sync-agent-detection.py --check
python -m unittest discover -s scripts -p 'test_sync_agent_detection.py'
```

On Windows, place Git for Windows' `usr/bin` directory on the test process's
PATH. Limit test threads when required by the local machine. Test with isolated
user data; `cargo dev` uses `.hitty-dev`. Native acceptance fixtures branch before
normal startup and use their own configuration directories.

## Native visual checks

```sh
cargo build --locked --features markdown-visual-tests --bin hitty-app
python scripts/prepare-readme-captures.py <new-temporary-directory>
```

Pass the generated document options to `hitty-app --config-dir <isolated-directory>
--document-visual <options.json>`. The fixture supports `panel_mode` values
`info`, `outline`, `changes` and `files`. Generated Markdown scenes leave the
mode to the document in front and show automatic Outline follow; source and
HTML scenes explicitly select `files`. Use the existing Markdown, Agent and
SSH visual scripts for their corresponding scenarios.

Capture a Windows application window with `scripts/capture-native-window.ps1`.
Use matching data, viewport, DPI, appearance and pointer state for Before/After.
Inspect saved images and include actual captures in PR evidence. A browser
prototype is design guidance, not proof of native application behavior.

## Release acceptance

Run the locked workspace/updater checks above, then verify the actual packages
on Windows x86_64, Linux x86_64, macOS ARM64 and macOS Intel. Check application
and CLI versions, command input/output, window close/reopen and session reconnect,
installation/removal where supported, Markdown rendering and native HTML preview
where available. Exercise updates with isolated user data and record platform
limitations. Verify all release assets and checksums using the
[release procedure](../maintenance/hitty-release.md).

## Current records

- [Herdr-core pane engine acceptance](herdr-pane-engine.md) and
  [two-axis pre-PR review](herdr-pane-engine-review.md).
- [README presentation](readme-product-presentation.md).
- [Compact right panel and file-tree alignment](right-panel-compact.md).
- [Outline follow for Markdown previews](outline-follow.md).
- [Agent pane task titles](agent-pane-task-titles.md),
  [notification task titles](agent-notification-task-title.md) and
  [session resume after restart](agent-resume-after-restart.md).
- [Original Hitty themes](theme-removal.md).

Superseded prototypes, early release acceptance bundles and old README images
are available in Git history. Published release notes remain in `docs/releases/`.

Keep new verification records concise and tied to reproducible commands. Avoid
copying per-run logs or completed design proposals into this directory. The
small fixture under `fixtures/` is compiled by an existing Agent detection test.

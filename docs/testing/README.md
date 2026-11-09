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
`info`, `outline`, `changes` and `files`. Use the existing Markdown, Agent and SSH
visual scripts for their corresponding scenarios.

Capture a Windows application window with `scripts/capture-native-window.ps1`.
Use matching data, viewport, DPI, appearance and pointer state for Before/After.
Inspect saved images and include actual captures in PR evidence. A browser
prototype is design guidance, not proof of native application behavior.

## Current records

- [v0.2.0 release verification](hitty-v0.2.0-release.md) and [published asset metadata](hitty-v0.2.0-release.json).
- [First release verification](hitty-bootstrap.md) and [release metadata](hitty-v0.1.0-release.json).
- [README presentation](readme-product-presentation.md).
- [Right-panel implementation](right-panel.md).

Keep new verification records concise and tied to reproducible commands. Avoid
copying per-run logs or completed design proposals into this directory. The
small fixture under `fixtures/` is compiled by an existing Agent detection test.

# Agent screen-detection manifests

These TOML files come from [herdr](https://github.com/herdrdev/herdr)
(`src/detect/manifests/`, commit `ee5bfb5041bb3e4a49d591c72308aae3c81ecdde`),
licensed under the Apache License 2.0. All 15 upstream files are copied
byte-for-byte. `upstream.json` records each source path, version and SHA-256.
Versions belong to individual manifests, not to the entire directory.
`copilot.toml` is herdr's `github-copilot.toml`, renamed after
tty7's slug.

`extensions/codex.toml` holds tty7's `ready_prompt` rule for the visible
input prompt and shortcuts footer. The engine appends it after upstream
Codex rules; working and approval rules retain their higher priority.
Keep local extensions separate when updating the upstream files.

`extensions/cursor.toml` adds rules for a running spinner or an approval
prompt after Herdr's `│` pane divider. The engine appends them to Cursor's
upstream rules with the same priorities and detection regions.

`extensions/claude.toml` covers named-session questions with Claude's U+25FE
active-task marker and trailing padding in terminal grid rows, both missed by
the pinned upstream rule. The upstream manifest remains byte-for-byte intact.

Check the complete set offline with Python 3.11 or newer:

```sh
python scripts/sync-agent-detection.py --check
```

Sync every supported agent at one full upstream commit SHA:

```sh
python scripts/sync-agent-detection.py --update <40-character-commit-sha>
```

The updater fetches and validates the entire set before writing any file.
CI checks coverage and content hashes, and tests that partial updates and
engine incompatibilities fail. Additions must also be registered in
`manifest_source` in `src/terminal/screen_status.rs`.

Each file lists rules that classify a coding agent's pane as `working`,
`blocked` or `idle`, based on what the agent's TUI is showing at the bottom of
the screen. `src/terminal/screen_status.rs` evaluates them the way herdr's
`src/detect/manifest.rs` does.

The engine supports manifest version 3 semantics. Screen text, the current
OSC title, and OSC 9;4 progress reports are separate detection inputs. A rule
requiring a newer engine is rejected. tty7 keeps its hook/session ownership
and completion debounce behavior; this sync does not add new agent types or
remote rule downloads at runtime.

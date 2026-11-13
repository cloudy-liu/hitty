# Asking an agent in another pane (`hitty ask`, `hitty peers`)

Validated on Windows 11, 2026-11-13, branch `feat/agent-ask`, with a
task-specific target directory outside the repository.

## Automated

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo test --locked --workspace --features updater --no-fail-fast -- --test-threads=4` | 3,360 passed, 3 failed, 14 ignored (see below) |
| `cargo test --locked --features updater --bin hitty-app -- --test-threads=4` (rerun) | 1,871 passed, 0 failed, 7 ignored |
| `cargo test --locked -p hitty-cli --test cli_e2e` | 14 passed, including the new `ask` test |

The three failures in the first workspace run were in files this change does
not touch. `core::aumid::tests::the_test_binary_counts_as_build_output` needs
a `CACHEDIR.TAG` in the target directory, which the hand-made directory
lacked; with the tag it passes. The two `ui::tray::taskbar` tests talk to the
real Explorer taskbar; they pass alone on both the baseline and this branch,
and passed in the full rerun of the GUI test binary.

## End to end, isolated server

`ask_reaches_the_agent_on_the_right_and_returns_its_answer` in
`crates/hitty-cli/tests/cli_e2e.rs` drives the compiled `hitty` against a
throwaway server with its own config and data directories:

1. A raw-mode stand-in agent is spawned in a pane, filed to the right of the
   caller's pane, and reports its turns over the same socket message the hook
   command uses.
2. `hitty peers --json` from the caller lists it as `right`, `claude`, `idle`.
3. `hitty ask right` with a two-line message returns the stand-in's own
   answer. The answer is over 9,000 bytes, more than an OSC may carry, and
   states that the message arrived as one four-line paste marked as a peer's.
4. `hitty ask claude` while `done` still stands from the first turn returns
   the second answer, not the first.
5. `hitty wait right --json` carries the same `reply`; `hitty agents --json`
   does not; `hitty capture right --plain` shows the pane's screen without
   the answer in it.
6. `hitty ask left` exits 1 with `no pane left of`.

## Against the installed 0.4.0 server (read-only)

The built CLI was pointed at the running Hitty 0.4.0 server, which predates
the reply and turn fields. With `HITTY_PANE=47` (a pane whose right neighbour
runs Claude Code):

```text
$ hitty peers
PANE  WHERE  AGENT   STATUS   TITLE
%52   right  claude  done     ✳ Claude Code
%51   @3     claude  working  ◑ Codex 会话后续讨论
$ hitty ask left "x"
hitty: there is no pane left of %47 — `hitty peers` shows what is around it
```

`hitty capture right --plain` printed pane 52's screen.

## Not verified

- No message was sent to a real Claude Code or Codex session. That
  `last_assistant_message` reaches the `Stop` hook is taken from both agents'
  hook documentation; the hook and daemon handling are covered by unit tests
  with that payload shape.
- macOS and Linux were not run locally.
- Against a server older than this change, `ask` detects the end of a turn
  only if it sees the agent `working` between polls, and returns the screen
  tail rather than the agent's text.

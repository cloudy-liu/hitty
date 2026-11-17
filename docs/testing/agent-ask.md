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

## Status from hooks and screen (follow-up)

Validated on Windows 11, 2026-11-17, branch `feat/ask-screen-status`.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `python scripts/sync-agent-detection.py --check` | all 15 manifests verified |
| `bash .github/scripts/check-host-boundary.sh` | clean |
| `cargo test --locked --workspace --features updater --no-fail-fast -- --test-threads=4` | 3,371 passed, 0 failed, 14 ignored |

The CLI unit tests drive `ask` and `peers` with scripted screens taken from
the detection engine's own fixtures: a Cursor pane with no hooks is asked and
answered from its screen; a Codex approval prompt its hooks report as
`working` stops a send and ends a wait with exit 3; a hookless agent quit
mid-turn exits 1 instead of returning the shell prompt as an answer; a wait
that runs out as the screen settles returns the screen.

Against the running 0.4.0 server (read-only), the built CLI listed a Codex
pane as `done` in 0.9 s, agreeing with `hitty agents`.

Not verified: no message was sent to a live agent, and no live Cursor or
blocked Codex session was read; those paths rest on the scripted screens.

## Review regression fixes (2026-11-17)

- `a_replacement_session_cannot_answer_the_asked_turn` first failed by returning
  `unrelated answer` with exit 0. It now covers a different agent, another
  session of the same agent, and withdrawal of a known session ID. Each ends
  the wait with exit 1 and no answer from the replacement.
- The daemon answer regression first failed because a repeated Stop advanced
  the completion count from 1 to 2. A completion is now counted once until a
  new prompt/session changes the state. Duplicate reports retain answer text;
  a later full-text report can fill an answer the first report lacked.
- Another PromptSubmit followed by Stop still increments the count, including
  turns with no reply text. The 10 KB answer stays out of terminal replay.
- `cargo test --locked --workspace --features updater`: **3364 passed,
  0 failed, 14 ignored**. The existing isolated CLI e2e suite also passed;
  no live agent session was messaged or restarted.

## Resized screen regression fixes (2026-11-17)

The screen-status follow-up includes the session and completion fixes above.
The real capture observer replays every geometry segment, including an empty
segment started by a resize. Reading only that segment as a fresh terminal
lost the permission prompt that the actual terminal still displayed.

- Before the fix, `a_resized_permission_screen_is_not_mistaken_for_an_idle_agent`
  returned `sent to %52` and status `idle`. After the fix, both an empty resize
  segment and a cursor-only delta preserve `Allow write? (y) (enter)`: `ask`
  refuses as waiting on its user, and no input is sent.
- `a_full_repaint_after_resize_retires_the_old_permission_screen` verifies that
  an alternate-screen clear or a primary-screen viewport-and-history clear,
  followed by repainting, removes the old blocker and allows a send.
- On Windows, a permission row below the cursor survived shrinking in the
  application but was dropped by the default replay grid. The primary-screen
  regression first returned `sent to %52`; matching the application's ConPTY
  resize configuration now preserves the prompt and refuses the send.
- Status and fallback text now replay the bounded capture history through one
  terminal, preserving its parser, cells, cursor and alternate-screen state
  across geometry changes. `capture --plain` keeps its existing history view.
- All 22 focused `ask` tests passed, including session replacement, hookless
  completion, permission prompts and the resize regressions.
- `cargo test --locked --workspace --features updater`: **3375 passed,
  0 failed, 14 ignored**, including the isolated public CLI e2e suite.
- `cargo check --locked --workspace --all-targets --features updater`, format,
  all 15 manifest checks and the host-boundary check passed.

The daemon replay ring is bounded to 8 MiB. Replaying it per look costs more
than reading only the newest segment; state older than retained history cannot
be recovered. No live agent was messaged, and live Cursor/blocked Codex
acceptance remains unverified as recorded above.

The shared screen engine can conservatively match recent primary-screen
history. `CSI 2J` alone retains that history; a stale permission row there can
still cause a refusal, as it can in the window. `CSI 3J` clears that history.

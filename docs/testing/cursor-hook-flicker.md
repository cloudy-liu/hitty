# Cursor hook ownership acceptance

Verified on Windows on 2026-10-11 for cloudy-liu/ctty7#129 with Cursor Agent
`2026.10.01-e373342`, using the Claude Haiku 5.5 model. The application and
daemon were rebuilt from the PR branch. The existing installation on PATH
was deliberately retained as the Claude-compatible hook runner.

## Observed failure

The previous acceptance application at `7444350d` already contained the
BOM parsing fix. A real Cursor pane nevertheless emitted this OSC body:

```json
{"v":1,"agent":"claude","event":"tool-complete"}
```

The configured hook command was `tty7-app.exe agent-hook claude
tool-complete`. PATH resolved that name to the installed application rather
than the rebuilt acceptance application. That runner lost the native Cursor
caller/session fields. The daemon accepted the foreign event as a change of
identity, discarded the Cursor session, and notified the UI. A later process
probe restored Cursor, which reset the screen tracker again.

Windows Computer Use observed the real Agent executing four separate
read-only Git commands. In baseline frame 42, the sidebar changed to Claude
and `idle` while the Agent still displayed `Running` and `ctrl+c to stop`.
Frame 43 showed Cursor and `run` again.

![Baseline live sidebar observations](images/cursor-flicker-before-live.gif)

[Full baseline failure frame](images/cursor-flicker-before-live.png)

## Final behavior

The daemon preserves an existing pane owner when another agent sends a
status hook. A foreign `session-start` must carry a session ID before it can
replace that owner. This also covers the malformed startup event observed
during live validation. Valid session replacement, same-agent hooks,
agent-less hooks, and hook discovery in an unowned pane remain supported.

The final application was built on `649c9f9a` plus this ownership fix, using
the original PR branch. Computer Use observed 150 frames across startup and
two real Agent turns. Every observed avatar remained Cursor. The first turn
ran four independent commands, each preceded by `Start-Sleep -Seconds 2`:

- `git worktree list`
- `git branch --show-current`
- `git status --short`
- `git log -1 --oneline`

The second turn ran `git worktree list` and `git branch --list` separately.
The sidebar displayed `run` during work and returned to `idle` after each
completed response. No observed running frame showed the baseline's
Claude/idle substitution.

![Final live sidebar observations](images/cursor-flicker-after-live.gif)

[Full final completed response](images/cursor-flicker-after-live.png)

These GIFs contain native sidebar crops from successive Computer Use window
observations. They preserve the recorded observation intervals, including
gaps between observation batches. They are sampled observations, not an
uninterrupted video. The [timestamp record](cursor-flicker-observations.json)
lists every saved frame. The baseline contains 65 frames over 37.8 seconds;
the final run contains 150 frames over 215.4 seconds. Application pixels were
not redrawn. The native full-window captures remain available above.

## Regression and checks

`cursor_foreign_status_hooks_cannot_flicker_identity_or_reset_the_session`
feeds the captured missing-session OSC shape through `OscSniffer` and the
daemon signal handler, interleaved with real process-probe application. It
checks identity, complete session preservation, and absence of subscriber
notifications for foreign startup, tool, prompt, stop, and notification
events. Before the guard, it failed with `Some(Claude)` instead of
`Some(Cursor)`; after the guard, it passed.

Validation completed:

- All seven Cursor-focused core tests passed.
- `cargo test --workspace --locked` passed: 3021 tests, zero failures,
  ten existing ignored tests. Git for Windows' `usr/bin` was added to the
  test process PATH for the existing `cat` and `sleep` bridge tests.
- `cargo build --locked --bin tty7-app` passed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Independent standards and specification reviews found no actionable
  findings in the final production change.

Live visual acceptance covers this Windows/Cursor configuration. macOS and
Linux live Agent UI behavior was not observed in this session.

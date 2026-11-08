# Local terminal startup verification

PR: [cloudy-liu/ctty7#92](https://github.com/cloudy-liu/ctty7/pull/92).
Verified locally on Windows on 2026-10-06.

## Failure and change

The previous native build repeatedly reported `no answer to Spawn within 2s`
on first launch. Local shell creation ran on the UI thread and reused the
short Attach reply deadline, even though cold ConPTY creation can take longer.

Local shells now use the existing pending-pane path and background executor.
Spawn replies have a 15-second budget; Attach retains its existing budget.
Failed panes keep their retry action. Agent fork commands wait until the
terminal lands. Shared tree synchronization holds tabs containing a fresh
pending pane so it cannot publish a split as only its ready sibling.

## Verification

- The full locked Windows workspace suite passed 2,985 cases with eight existing ignored tests.
- A delayed daemon reply beyond the local Attach deadline succeeded, and a silent reply timed out without entering the disconnect retry path.
- GPUI tests checked failure/retry state, focus and workspace owner preservation, one queued command delivery and split tree preservation.
- The first macOS CI run exposed a fixture lifetime race: the mock daemon closed before the socket-timeout reset assertion. The fixture now keeps the connection open until that assertion completes; its delayed Spawn reply had already succeeded.
- The locked workspace build, formatting, whitespace, offline editor-palette regeneration and 98-file host-boundary check passed.
- Native startup produced a first shell without a new Spawn timeout. A new PowerShell tab and horizontal split showed the expected fixture directory.
- With session restoration enabled in the isolated test config, a GUI restart restored the same workspace and horizontal split with both prompts visible.
- All nine [CI jobs](https://github.com/cloudy-liu/ctty7/actions/runs/37450774839) passed on `ffa76e55`, including the corrected macOS fixture and the macOS/Linux native Markdown matrices; PR #92 merged as `57c7c511`.

## Limits

Native SSH retains its initial synchronous handshake. The pending-slot close
path already kills late-arriving panes, but the native smoke check did not
inject a delayed result into a closed slot. Native visual inspection was on
Windows at the existing 175% display scale; cross-platform CI is the build and
test record for macOS/Linux.

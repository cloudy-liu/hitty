# Agent notification task titles

November 12, 2026. Baseline: `d82c162`.

Agent notifications now use the terminal title written by the source Agent.
The latest OSC title is read before the tab's title-settling delay expires.
Empty titles and the initial `hitty` title keep the Agent-name fallback.
The existing workspace suffix, event body, notification policy, sound and
source-pane click target are unchanged.

## Regression evidence

The existing lifecycle harness drives two panes running the same Agent through
working, done and waiting. It checks different Chinese task names, pending OSC
titles, source pane identities and the empty/default title fallback.

```powershell
cargo test --locked --bin hitty-app agent_notifications_name_the_source_task
```

Before the fix, the assertion failed:

```text
left: "Claude Code"
right: "修复登录回调"
```

After the fix, the regression passed within the full locked workspace and
updater suite: **3,345 passed, 0 failed, 14 existing ignored cases**.

```powershell
cargo fmt --all -- --check
cargo test --locked --workspace --features updater -- --test-threads=4
cargo build --locked --workspace --bins --features updater,markdown-visual-tests
```

Formatting and the final locked workspace build passed. Full local logs are
`D:/tmp/hitty-notification-tests.log` and `D:/tmp/hitty-notification-build.log`.

## Native acceptance

The existing `--agent-attention-visual` fixture has an opt-in `notifications`
setting. Its default behavior remains silent. Both stages use separate
temporary profiles, the same two Claude Code source panes (`Agent A` and
`Agent B`), a horizontal split, 1120 × 640 logical windows, the light theme,
desktop notifications set to Always, and Agent sounds disabled. No live model
or shell is involved. The application sends real Windows toast notifications.

Build each stage with `cargo build --locked --features markdown-visual-tests
--bin hitty-app`, and run:

```powershell
hitty-app.exe --config-dir <isolated-profile> --agent-attention-visual <options.json>
```

The options file names the fixture's control and state files:

```json
{
  "control": "<isolated-profile>/control.json",
  "state": "<isolated-profile>/state.json",
  "split": true,
  "tray_disabled": true,
  "notifications": true
}
```

After both source panes report working, write this UTF-8 JSON control file:

```json
{"sequence":1,"reports":[[0,0,"done"],[0,1,"done"]]}
```

The [Before record](agent-notification-task-title-before.json) contains the
baseline binary hash, fixture state and two actual toast XML documents read
through `ToastNotificationManager.History.GetHistory("com.github.hitty")`.
Both titles are `Claude Code` even though the source titles differ.

The [After record](agent-notification-task-title-after.json) confirms both
source panes reached done and Windows received two distinct notification
titles. The elapsed durations reflect when each fixture was triggered.

| Source terminal title | Before toast title | After toast title |
| --- | --- | --- |
| Agent A | Claude Code | Agent A |
| Agent B | Claude Code | Agent B |

**Visual acceptance is unverified.** LCU stopped this turn because it could
not determine the current browser URL confidently enough to enforce policy.
No Before/After screenshot or native notification click was captured. Windows
notification-history evidence verifies delivered content, not visual layout
or click behavior. Keep the PR draft until matching real toast captures and
click acceptance are completed or explicitly accepted by the maintainer.

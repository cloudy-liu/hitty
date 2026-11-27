# Changelog

## v0.5.4 - 2026-11-27

- Open the current working directory from Info and collapse long Agent
  conversations independently of Outline (cloudy-liu/hitty#67).
- Reuse a compatible Hitty Server for eligible SSH tabs and reconnect
  to the same remote terminal after a transport loss (cloudy-liu/hitty#68).
- Keep pinned directories visible when they have no sessions and launch
  Agents from the sidebar with configurable commands (cloudy-liu/hitty#70).

See the [release notes](docs/releases/hitty-v0.5.4.md) for scope and
limitations.

## v0.5.3 - 2026-11-26

- Show how long a session has been quiet. Track typing, new agent text, and
  agent events. Hide ages under one minute and keep the compact age in the
  hover card (cloudy-liu/hitty#64).
- Restore standard agent icons by default and apply per-agent icon choices
  across session navigation and pane titles. Improve the clarity of bundled
  icons on scaled displays (cloudy-liu/hitty#63).

See the [release notes](docs/releases/hitty-v0.5.3.md) for scope and
limitations.

## v0.5.2 - 2026-11-25

- Add per-agent sidebar icon customization with bundled product artwork for
  Claude Code, Codex, Antigravity, and Qoder, alias search, and an independent
  toggle for showing agent names in the sidebar (cloudy-liu/hitty#59,
  cloudy-liu/hitty#61).
- Keep sidebar activity durations and calendar dates on a single line across
  sidebar widths and UI font sizes, and show the compact activity label beside
  the full title in the hover card (cloudy-liu/hitty#60).

See the [release notes](docs/releases/hitty-v0.5.2.md) for scope and
limitations.

## v0.5.1 - 2026-11-23

- Show recent agent activity beside session titles. Hover to see the exact
  local time, the activity type, and the full branch name when it was shortened.
  Recorded activity survives a restart (cloudy-liu/hitty#57).

See the [release notes](docs/releases/hitty-v0.5.1.md) for scope and
limitations.

## v0.5.0 - 2026-11-20

- Keep Git Diff review controls fixed beside the file list and add one-button
  collapse/expand for file cards while preserving the reading position
  (cloudy-liu/hitty#55).
- Preserve Codex's native prompt-history and input colors when switching
  application themes, including One Light and One Dark Pro
  (cloudy-liu/hitty#50).
- Restore saved local Qoder and Qoder CN conversations without hooks, retaining
  each pane's exact conversation and configuration directory
  (cloudy-liu/hitty#51).

See the [release notes](docs/releases/hitty-v0.5.0.md) for scope and
limitations.

## v0.4.1 - 2026-11-17

- Read one fresh configuration snapshot for each pane and prepare complete
  command history alongside shell creation, reducing repeated startup work
  and moving history preparation off the UI thread (cloudy-liu/hitty#47).
- Add reproducible startup measurements and isolated Cmder comparison tools;
  the tools apply candidate changes only to generated copies
  (cloudy-liu/hitty#47).

See the [release notes](docs/releases/hitty-v0.4.1.md) for measurement scope and
distribution limitations.

## v0.4.0 - 2026-11-12

- Open Windows sessions and splits without the three-second pause before the
  shell starts: the daemon now answers the pseudoconsole's startup query itself
  (cloudy-liu/hitty#40).
- Show a Markdown preview's outline in the open right panel when the document
  has two or more headings, and return to the chosen mode when it closes
  (cloudy-liu/hitty#39).
- Add an optional setting that shows the task title an agent writes for its
  session after its name in the agent pane title row. Off by default
  (cloudy-liu/hitty#41).
- Reopen each Codex conversation in its own pane after a Windows update or
  restart, including two clients working in the same directory
  (cloudy-liu/hitty#37).
- Draw hover tooltips smaller and closer to the control they belong to, and
  show a control's shortcut in a quieter style beside its name
  (cloudy-liu/hitty#38).
- Remove the remaining Otty Solarized Light preset and its theme-style
  integration, restoring Hitty's original 13 built-in themes
  (cloudy-liu/hitty#42).

See the [release notes](docs/releases/hitty-v0.4.0.md) for the upgrade note and
limitations.

## v0.3.1 - 2026-11-10

- Keep Hitty's interface and terminal typography when selecting themes, retain
  only Solarized Light from the imported theme catalogue, and prevent imported
  fonts from remaining after switching themes (cloudy-liu/hitty#34).
- Shorten executable-only Command Prompt titles to `cmd` in session navigation
  and CLI labels (cloudy-liu/hitty#35).

See the [release notes](docs/releases/hitty-v0.3.1.md) for the upgrade note and
limitations.

## v0.3.0 - 2026-11-10

- Add an optional herdr-core engine for local panes, with agent messaging,
  terminal recovery and native fallback (cloudy-liu/hitty#30).
- Add all 25 Otty theme profiles with authored terminal colors, transparent
  surfaces, native material requests, styled tabs and terminal cards;
  full native visual parity remains incomplete (cloudy-liu/hitty#31).
- Match right-panel mode icons and labels to the surrounding toolbar size
  (cloudy-liu/hitty#29).
- Present a single file-tree root as a title and remove its redundant child
  indentation while preserving folding and file actions (cloudy-liu/hitty#29).
- Indicate hidden-file visibility with open/crossed-out eye icons instead of
  a persistent selected background (cloudy-liu/hitty#29).
- Resolve GitHub commit-author avatars for ordinary email addresses
  (cloudy-liu/hitty#27).

See the [release notes](docs/releases/hitty-v0.3.0.md) for setup and limitations.

## v0.2.0 - 2026-11-09

- Refine the right panel into Info, Outline, Git and Files with compact mode
  labels and visible file-view actions.
- Navigate document headings, Agent turns and recorded terminal commands within
  their existing sessions.
- Review and prune obsolete inherited documentation.

## v0.1.0 - 2026-11-08

First independent Hitty release for Windows, macOS and Linux. Establishes Hitty
application identities, fresh settings and its own update/release line while
retaining the reviewed terminal workspace implementation.

See the [release notes](docs/releases/hitty-v0.1.0.md) and
[source provenance](docs/provenance/source-baseline.md). Earlier project release
history belongs to its original source and remains in the imported Git snapshot.

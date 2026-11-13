# Changelog

## Unreleased

- Add `hitty ask` and `hitty peers`, so an agent can be told "ask the Claude on
  the right" in words: panes resolve by side or agent name, the message is
  delivered only to an agent that is free, and its answer comes back. Claude
  Code and Codex return their own final text; reinstall agent hooks to enable
  it. `capture`, `wait` and `procs` accept the same pane words.

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

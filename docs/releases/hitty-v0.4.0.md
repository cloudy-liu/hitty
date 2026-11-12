# Hitty v0.4.0

Windows shells that start without a three-second wait, an outline that follows
the Markdown you are reading, agent task titles in split panes, and Hitty's
original themes.

Upgrade note: Solarized Light and the Otty style rendering are removed. A saved
Solarized Light selection falls back to Hitty Light. Choose one of the 13
original Hitty themes in settings (cloudy-liu/hitty#42).

## Highlights

- Start a new Windows session or split and its shell runs at once instead of
  after a three-second pause. On the reporting machine a Cmder shell reached
  its first prompt in about 0.5 seconds instead of 3.5 (cloudy-liu/hitty#40).
- Open a Markdown document with the right panel open and its outline appears
  without being asked. The view you chose returns when the document closes
  (cloudy-liu/hitty#39).
- Find your Codex conversations where you left them after a Windows update or
  restart, even when two of them share a directory (cloudy-liu/hitty#37).
- See what each agent in a split is working on, with an optional task title
  after its name (cloudy-liu/hitty#41).

## Changes

### Added

- Show a Markdown preview's outline in the open right panel when the document
  has two or more headings. A closed panel stays closed, a document with a
  single heading leaves the panel as it is, and a view you choose while reading
  is kept until that session has no open file left (cloudy-liu/hitty#39).
- Add the **Show the agent's task title in pane titles** setting under Window &
  Tabs, off by default. With **Show agent pane titles** on, it adds the title
  the agent writes for its session after its name in the pane title row
  (cloudy-liu/hitty#41).

### Changed

- Restore Hitty's original 13 built-in themes and their rendering. Imported
  nested `style` declarations no longer affect rendering. Original YAML themes
  and iTerm2 imports remain supported (cloudy-liu/hitty#42).
- Draw hover tooltips with smaller text and padding, 6 px from the control they
  belong to, and show a control's shortcut muted beside its name. Tooltips that
  follow the pointer keep their distance from it (cloudy-liu/hitty#38).

### Fixed

- Answer the Windows pseudoconsole's startup query from the daemon, so a new
  pane's shell is no longer held back for three seconds
  (cloudy-liu/hitty#40).
- Save each agent conversation's ID while it runs in a native Windows shell
  pane, so an update or restart reopens the same conversations instead of
  plain shells, including two Codex clients in one directory
  (cloudy-liu/hitty#37).

## Known limitations

- The faster shell start applies to panes on Hitty's built-in pane engine.
  Herdr-core panes were not measured, and the timings were taken at the daemon
  rather than in the desktop window (cloudy-liu/hitty#40).
- Exact conversation restore needs `restore_agent_sessions` enabled and a
  `codex` started by name in a native Windows shell pane. An absolute path or
  an alias bypasses it. A physical reboot and the update dialog were not
  exercised, and Claude Code was covered by a fixture rather than a live client
  (cloudy-liu/hitty#37).
- Native visuals for the outline follow, tooltips, pane task titles and theme
  removal were checked on Windows at 175% scale. Other Windows scales and
  native macOS/Linux visuals remain unverified (cloudy-liu/hitty#38,
  cloudy-liu/hitty#39, cloudy-liu/hitty#41, cloudy-liu/hitty#42).
- Herdr-core remains optional and separately installed. Its real runtime
  acceptance covers Windows; macOS/Linux hc runtime acceptance, remote
  workspaces and multi-viewer arbitration remain outside that integration
  (cloudy-liu/hitty#30).
- Windows packages are unsigned. macOS packages use ad hoc signatures and
  are not notarized; operating-system download protection may require an
  explicit first-launch exception.
- Linux retains its HTML source view rather than an embedded HTML preview.

**Full changelog:** [v0.3.1...v0.4.0](https://github.com/cloudy-liu/hitty/compare/v0.3.1...v0.4.0)

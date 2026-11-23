# Hitty v0.5.1

Find your most recently active agent sessions at a glance in the sidebar.

## Highlights

- See compact times such as `3m`, `1h`, and `2d` beside session titles. Hover
  to see the exact local time and what happened (cloudy-liu/hitty#57).

## Changes

### Added

- Update session activity when the agent receives a prompt, asks for input,
  or finishes a turn. Hitty can also record supported status changes that it
  reads from the live terminal screen. Recorded times survive a restart
  (cloudy-liu/hitty#57).

### Changed

- Share the right end of the title row between the timestamp and the close
  button. Agent status, branch, and change counts stay together on the second
  line. The hover card shows the full branch name when it is shortened
  (cloudy-liu/hitty#57).

## Known limitations

- Sessions display a time after Hitty records their first activity. Screen-based
  detection depends on what Hitty can read from the terminal, and remote panes
  need a current server to report those observations (cloudy-liu/hitty#57).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized. Operating-system download protection may require an explicit
  first-launch exception.
- Linux opens HTML as source. Herdr-core is optional and installed separately.

**Full changelog:** [v0.5.0...v0.5.1](https://github.com/cloudy-liu/hitty/compare/v0.5.0...v0.5.1)

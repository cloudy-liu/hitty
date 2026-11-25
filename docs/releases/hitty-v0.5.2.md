# Hitty v0.5.2

Customize sidebar agent icons individually, show optional agent names, and keep
activity labels on a single line.

## Highlights

- Choose sidebar icons per agent in **Settings → Window & Tabs → Sidebar agent
  icons**, with common agents listed first and alias search such as `agy` and
  `cbc` (cloudy-liu/hitty#59, cloudy-liu/hitty#61).
- Keep compact sidebar activity durations and dates on one line without
  increasing row height (cloudy-liu/hitty#60).

## Changes

### Added

- Add per-agent sidebar icon customization with live previews, command and
  alias search, and independent saving for each agent (cloudy-liu/hitty#61).
- Bundle product artwork for Claude Code (Clawd mascot, with Starburst still
  selectable and optional local `agent-icons/claude.png` override), Codex (app
  icon), Antigravity (Google's full-color mark), and Qoder (green CLI startup
  mark) (cloudy-liu/hitty#59, cloudy-liu/hitty#61).
- Add an independent **Show agent names in the sidebar** toggle alongside
  status and branch or directory, migrating existing preferences from the
  earlier combined toggle (cloudy-liu/hitty#59, cloudy-liu/hitty#61).

### Changed

- Trim rarely used entries from the sidebar icon picker and omit Droid and
  Goose from the Agents Hooks settings list while keeping existing Hooks
  configurations usable (cloudy-liu/hitty#61).
- Place the compact activity label at the right of the full title in the
  sidebar hover card alongside expanded Git and session details
  (cloudy-liu/hitty#60).

### Fixed

- Prevent sidebar activity durations and calendar dates from wrapping onto a
  second line or expanding row height, including in narrow sidebars and at
  large UI font sizes (cloudy-liu/hitty#60).

## Known limitations

- Native visual acceptance for the sidebar icon settings and single-line
  activity layout covers Windows (cloudy-liu/hitty#59, cloudy-liu/hitty#60,
  cloudy-liu/hitty#61).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized. Operating-system download protection may require an explicit
  first-launch exception.
- Linux opens HTML as source. Herdr-core is optional and installed separately.

**Full changelog:** [v0.5.1...v0.5.2](https://github.com/cloudy-liu/hitty/compare/v0.5.1...v0.5.2)

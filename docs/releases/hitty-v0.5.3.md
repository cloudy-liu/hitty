# Hitty v0.5.3

See how long sessions have been quiet and use sharper agent icons across
session navigation and pane titles.

If you used the bundled product icons that v0.5.2 selected by default,
re-enable the artwork you prefer in **Settings → Window & Tabs → Sidebar agent
icons** (cloudy-liu/hitty#63).

## Highlights

- The sidebar leaves the age blank for one minute after activity. It then
  shows the time since the last terminal input, new agent text, or agent
  event, and keeps that age in the hover card (cloudy-liu/hitty#64).
- Standard agent marks are the default again. Custom artwork carries through
  to session navigation and pane titles. The Qoder and Qoder CN options share
  a crisp green CLI mark (cloudy-liu/hitty#63).

## Changes

### Changed

- Reserve consistent space for the activity age in the sidebar. When a longer
  date has too little room in the row, show it in the hover card
  (cloudy-liu/hitty#64).
- Use the configured agent icon across the sidebar, tab strip, switcher, and
  pane titles (cloudy-liu/hitty#63).

### Fixed

- Ignore timer-only terminal redraws, passive input, and terminal replies when
  recording session activity (cloudy-liu/hitty#64).
- Improve the rendering of bundled Antigravity, Codex, and Qoder icons at
  fractional display scales (cloudy-liu/hitty#63).

### Internal

- Cover the `28m` activity label with a Windows regression scene and native
  evidence that the label stays on one line (cloudy-liu/hitty#65).

## Known limitations

- Native visual acceptance for the sidebar icons and activity labels covers
  Windows (cloudy-liu/hitty#63, cloudy-liu/hitty#64, cloudy-liu/hitty#65).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized. Operating-system download protection may require a first-launch
  exception.
- Linux opens HTML as source. Herdr-core is optional and installed separately.

**Full changelog** [v0.5.2...v0.5.3](https://github.com/cloudy-liu/hitty/compare/v0.5.2...v0.5.3)

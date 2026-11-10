# Hitty v0.3.1

Consistent theme typography, a focused theme catalogue, and concise Command
Prompt session titles.

Upgrade note: the 24 unselected Otty profiles from v0.3.0 are removed. A saved
selection of a removed profile falls back to Hitty's default theme. Choose
Solarized Light or an original Hitty preset in settings (cloudy-liu/hitty#34).

## Highlights

- Keep your configured font sizes and Hitty's scalable interface text when
  selecting themes or switching back to an original preset (cloudy-liu/hitty#34).
- Use Solarized Light alongside the 13 original Hitty themes, with no source
  suffix in the theme picker (cloudy-liu/hitty#34).
- See `cmd` instead of the Command Prompt executable path in session
  navigation and CLI labels (cloudy-liu/hitty#35).

## Changes

### Changed

- Retain only Solarized Light from the imported theme catalogue and prevent
  the importer from restoring the other 24 profiles. Its stored identifier
  remains compatible with existing selections (cloudy-liu/hitty#34).
- Use Hitty's font preferences, interface text sizes and native HiDPI path
  for imported and user-authored style profiles. Source font declarations
  remain metadata (cloudy-liu/hitty#34).

### Fixed

- Prevent theme font overrides from shrinking terminal or sidebar text and
  leaving interface typography behind after switching themes or cancelling
  a preview (cloudy-liu/hitty#34).
- Shorten executable-only Command Prompt titles to `cmd`, preserving task
  titles, user-assigned session names and the raw title in tooltips
  (cloudy-liu/hitty#35).

## Known limitations

- Native theme visuals were checked on Windows at 175% scale. Other Windows
  scales and native macOS/Linux theme visuals remain unverified
  (cloudy-liu/hitty#34).
- Herdr-core remains optional and separately installed. Its real runtime
  acceptance covers Windows; macOS/Linux hc runtime acceptance, remote
  workspaces and multi-viewer arbitration remain outside that integration
  (cloudy-liu/hitty#30).
- Windows packages are unsigned. macOS packages use ad hoc signatures and
  are not notarized; operating-system download protection may require an
  explicit first-launch exception.
- Linux retains its HTML source view rather than an embedded HTML preview.

**Full changelog:** [v0.3.0...v0.3.1](https://github.com/cloudy-liu/hitty/compare/v0.3.0...v0.3.1)

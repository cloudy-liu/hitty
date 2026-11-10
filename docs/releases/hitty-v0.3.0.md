# Hitty v0.3.0

Optional agent messaging through herdr-core, 25 Otty theme profiles, and more
compact file navigation.

The native pane engine remains the default. To use the optional herdr-core
engine, install a compatible `hc` separately (minimum version `0.1.15`) and set
`"pane_engine": "herdr-core"` in the profile's `config.json`. The selection
applies to newly created local panes (cloudy-liu/hitty#30).

## Highlights

- Let agents in local panes address each other with `hc send` and wait for
  child-agent results while keeping Hitty's existing interface
  (cloudy-liu/hitty#30).
- Choose from 25 additional Otty profiles, with authored colors and supported
  styling for transparent surfaces, tabs, terminal cards and typography
  (cloudy-liu/hitty#31).
- Use compact right-panel controls, a clearer file-tree root and simpler
  hidden-file visibility indicators (cloudy-liu/hitty#29).

## Changes

### Added

- An optional herdr-core engine for local panes, including stable terminal
  identities, messaging, daemon reconnect and recovery, and native fallback
  when the installed core is unavailable or incompatible (cloudy-liu/hitty#30).
- All 25 Otty theme profiles alongside the original 13 presets. Imported styles
  survive duplication and editing, with a separate readable-colors preference
  (cloudy-liu/hitty#31).

### Changed

- Smaller right-panel mode icons and labels, a single file-tree root presented
  as a title, and less indentation beneath it (cloudy-liu/hitty#29).
- Open and crossed-out eye icons indicate hidden-file visibility without a
  persistent selected background (cloudy-liu/hitty#29).

### Fixed

- GitHub author avatars now resolve for commits using ordinary email addresses
  (cloudy-liu/hitty#27).

### Internal

- Broader workspace CI coverage, a stable selected-commit rendering test,
  separate debug-build caches and more headroom for cold Intel macOS builds
  (cloudy-liu/hitty#30, cloudy-liu/hitty#31).

## Known limitations

- Herdr-core is optional and is not bundled, installed or upgraded by Hitty.
  Real hc runtime acceptance covers Windows; macOS and Linux hc runtime
  acceptance, remote workspaces and multi-viewer arbitration remain outside
  this first integration (cloudy-liu/hitty#30).
- Otty theme reproduction is incomplete. Windows acrylic tint and luminosity
  are approximated; macOS materials use the existing blur backend. Native
  material parity, text rasterization, bottom-tab layout and popup-shadow
  parity remain unverified or incomplete (cloudy-liu/hitty#31).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized; operating-system download protection may require an explicit
  first-launch exception.
- Linux retains its HTML source view rather than an embedded HTML preview.

**Full changelog:** [v0.2.0...v0.3.0](https://github.com/cloudy-liu/hitty/compare/v0.2.0...v0.3.0)

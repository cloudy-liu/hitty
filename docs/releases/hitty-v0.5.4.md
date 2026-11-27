# Hitty v0.5.4

Open working directories from Info, launch Agents from pinned folders, and
keep eligible SSH terminals running on Hitty Server when a connection drops.

## Highlights

- Use the Info panel to copy or open the current working directory and fold
  long Agent conversations (cloudy-liu/hitty#67).
- Open a saved SSH host in a local workspace and reuse a compatible Hitty
  Server when available. The sidebar shows the remote Agent, and reconnecting
  restores the same live terminal (cloudy-liu/hitty#68).
- Keep pinned local directories visible with zero sessions. Use the directory
  menu to open a shell or launch an installed Agent (cloudy-liu/hitty#70).

## Changes

### Added

- Choose and remember an external editor from Info. The folder button opens
  the local file manager, and the new-tab button starts a terminal at the
  working directory for supported sessions (cloudy-liu/hitty#67).
- Set per-Agent quick launch commands in **Settings → Agents** and reset them
  to the bundled defaults (cloudy-liu/hitty#70).
- See **Hitty Server** or **SSH shell** as the connection type in Info
  (cloudy-liu/hitty#68).

### Changed

- Use screen-based status for Copilot CLI, Gemini, and Qwen Code. Hitty keeps
  recognizing those Agents and removes their hook installers from Settings
  (cloudy-liu/hitty#69).
- Keep Outline expanded when the conversation in Info is folded
  (cloudy-liu/hitty#67).

### Fixed

- Pass Windows editor arguments directly to native executables so names with
  spaces or percent signs arrive intact (cloudy-liu/hitty#67).
- Preserve an explicit managed SSH close request through transient connection
  failures while the local daemon remains running (cloudy-liu/hitty#68).
- Show Hitty in the original pixel lettering on the empty workspace
  (cloudy-liu/hitty#70).

## Known limitations

- SSH sessions fall back to a regular shell when a compatible server is
  unavailable or the profile requires agent forwarding, X11 forwarding,
  a custom terminal type, or disabled shell integration (cloudy-liu/hitty#68).
- A managed pane can reattach while its owning remote daemon and pane are
  still running. Open a fresh SSH tab if that remote pane is gone or its
  daemon has restarted (cloudy-liu/hitty#68).
- Re-create port forwards after a managed SSH transport reconnect. If you
  roll back Hitty, restart the local daemon with the older build as part of
  the rollback (cloudy-liu/hitty#68).
- Windows **Open in** requires a native editor executable. A batch-only
  launcher cannot open a directory from Info (cloudy-liu/hitty#67).
- Windows packages are unsigned. macOS packages use ad hoc signatures and are
  not notarized. Operating-system download protection may require a first-launch
  exception.
- Linux opens HTML as source. Herdr-core is optional and installed separately.

**Full changelog** [v0.5.3...v0.5.4](https://github.com/cloudy-liu/hitty/compare/v0.5.3...v0.5.4)

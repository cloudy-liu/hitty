# Hitty v0.1.0

Hitty's first independent release establishes a terminal workspace for coding
agents with its own application identity, command line, settings, and updates.

## Highlights

- Use `hitty` for the command line and Hitty for the desktop application.
- Start with fresh Hitty settings and the existing application defaults.
- Recognize Hitty across installers, desktop integration, remote services,
  documentation, and the approved light/dark logo.

## Changes

- Renamed application-owned binaries, packages, environment variables, hooks,
  configuration paths, and remote services consistently to Hitty.
- Directed application and remote-server downloads to `cloudy-liu/hitty`.
- Established the `0.1.0` version line with independent project history.
- Fixed Windows uninstall cleanup terminating the installer process that
  invoked it, while continuing to stop application processes holding files open.
- Retained terminal workspaces, persistent sessions, SSH/WSL connections,
  document previews, coding-agent integrations, and the original application icon.

## Installation notes and limitations

Hitty is a fresh installation. It does not import another product's settings,
credentials, workspaces, or live sessions. Customize your new installation
through Hitty's settings.

Windows packages are unsigned. macOS packages use ad-hoc signatures and
have not been notarized. Operating-system download protection may prevent
an ordinary first launch.

Create SSH and WSL connections through the desktop interface. The CLI's
`machine connect` and `machine disconnect` commands are not implemented.

## Source baseline

Hitty builds on [tty7](https://github.com/l0ng-ai/tty7). The reviewed source
snapshot is preserved in Hitty's
[source import commit](https://github.com/cloudy-liu/hitty/commit/4699e7eb4294883c324bf0c630bcde22fd709946),
with the original licenses and attribution retained. The exact export revision
and import details are recorded in
[source provenance](https://github.com/cloudy-liu/hitty/blob/main/docs/provenance/source-baseline.md).
Hitty has independent Git and release history.
Subsequent Hitty release comparisons will use Hitty tags.

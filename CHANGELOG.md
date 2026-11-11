# Changelog

## Unreleased

- Keep all Otty themes on Hitty's interface and terminal typography settings,
  and prevent imported fonts from remaining after switching themes.
- Draw hover tooltips smaller and closer to the control they belong to, and
  show a control's shortcut in a quieter style beside its name.

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

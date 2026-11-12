# Hitty identity and source decisions

Hitty began as an independent source snapshot, recorded in
[ADR 0001](adr/0001-independent-source-snapshot.md) and
[source provenance](provenance/source-baseline.md). The separate import commit
`4699e7e` preserves the reviewed tree before branding changes.

- The first release is Hitty `v0.1.0`; subsequent versions follow Hitty's tags.
- Application-owned identities use Hitty, `hitty` and `HITTY` consistently.
- Hitty starts with its own user-data directories and source-code defaults.
  There is no legacy configuration import or adoption of live sessions.
- The maintainer's personal shell, font and language choices are not product defaults.
- README and documentation use the approved double-pane icon plus rounded Hitty
  wordmark. The standalone application/tray artwork retains its accepted design.
- Preserve inherited functionality, protocol contracts, pinned dependency versions,
  original licenses and truthful source attribution.
- Validate changed public behavior using isolated data and inherited test/capture
  harnesses; keep unrelated feature work separate.

Windows, macOS and Linux `v0.1.0` were published together on November 8, 2026.
[Release notes](releases/hitty-v0.1.0.md) record the first release's scope and known
distribution limitations. Use the current [release procedure](maintenance/hitty-release.md)
for subsequent work; do not overwrite published tags or binaries.

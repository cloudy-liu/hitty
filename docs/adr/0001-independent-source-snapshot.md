# Start Hitty from an independent source snapshot

Status: implemented. The import and first release are recorded in
[source provenance](../provenance/source-baseline.md) and
[first release notes](../releases/hitty-v0.1.0.md).

Hitty imported a reviewed ctty7 source snapshot after its README bootstrap
commit, without importing the old repository's Git history, branches, tags,
issues, pull requests or releases. This gives Hitty its own development history
and release line. The source record retains the exact revision, licenses and
attribution; the original repository remains the reference for earlier development.

## Consequences

- Keep the application source baseline separate from subsequent branding changes and larger refactors so changes can be compared against a known starting point.
- Preserve the README bootstrap commit; the source import is the first application-code commit, not the repository's root commit.
- The selected source revision is `7f430a8f5f89a3a2963250b196b0baea02569299` in ctty7. Subsequent rebranding and fixes belong in Hitty.

This records the maintainer's decision from the preceding discussion and the accepted scope of cloudy-liu/hitty issue #1. Earlier recommendations to rename the old repository or retain its Git history are superseded.

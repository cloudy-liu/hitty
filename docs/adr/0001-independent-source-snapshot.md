# Start Hitty from an independent source snapshot

Hitty will import a reviewed ctty7 source snapshot after its existing README bootstrap commit, without importing the old repository's Git history, branches, tags, issues, pull requests, or releases. We chose this over renaming ctty7 or retaining its Git ancestry because Hitty needs its own development history, project space, and release line ahead of substantial future changes. The ctty7 repository remains the reference for earlier development; the import will record its exact source revision and retain existing license and attribution material.

## Consequences

- Keep the application source baseline separate from subsequent branding changes and larger refactors so changes can be compared against a known starting point.
- Preserve the existing README bootstrap commit; the source import will be the first application-code commit, not the repository's root commit.
- The selected source revision is `7f430a8f5f89a3a2963250b196b0baea02569299` in ctty7. Subsequent rebranding and fixes belong in Hitty. Creating the repository and its specification does not mean the application has been imported or released.

This records the maintainer's decision from the preceding discussion and the accepted scope of cloudy-liu/hitty issue #1. Earlier recommendations to rename the old repository or retain its Git history are superseded.

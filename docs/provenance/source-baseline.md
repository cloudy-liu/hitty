# Source baseline

Hitty starts from the tracked source tree of
`cloudy-liu/ctty7` at `7f430a8f5f89a3a2963250b196b0baea02569299`.
The original project originated from `l0ng-ai/tty7`.

The source was exported with `git archive`; no Git ancestry, tags, branches,
issues, pull requests, local configuration, build outputs, or runtime state
were imported. The source tree contained 1,535 files. Of those, 1,534 were
imported and compared byte-for-byte with the archive. There were no submodules,
symlinks, export exclusions, or unresolved Git LFS pointers in that snapshot.

The source README is intentionally replaced by Hitty's existing project
introduction. Hitty's glossary, approved bootstrap plan/specification, source
decision record, this provenance record, and contributor guidance are project
metadata additions. The README bootstrap commit remains in Hitty's independent
history. The source baseline commit retains the application's original code
and identities so subsequent rebranding can be reviewed separately.

Original license, copyright, and third-party attribution material is retained.
Earlier implementation history and historical issue references belong to their
actual source repositories. Imported implementation is not newly authored code.

The following external dependency references are historical source facts:

- The gpui dependency is a pinned fork of Zed in `cloudy-liu/zed`; source
  comments referred to its `tty7` branch. The manifest uses a fixed revision.
- The terminal dependency is the pinned `l0ng-ai/alacritty` fork, and the
  component dependency is pinned in `cloudy-liu/gpui-component`.
- The selected source includes prior ctty7 modifications. Those names in
  provenance are not active Hitty product identities.

Selected Hitty branding studies were uncommitted in the source workspace and
are handled separately from this tracked source baseline.

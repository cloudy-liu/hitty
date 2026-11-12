# Hitty release procedure

Use `cloudy-liu/hitty` and Hitty tags.
For `v0.4.0`, compare against `v0.3.1`, published on November 10, 2026.
Always compare a later release against the immediately preceding published
Hitty tag.

1. Complete the checks in `docs/testing/README.md` and the agreed
   Windows x86_64, Linux x86_64, macOS ARM64, and macOS Intel acceptance gate.
   Follow the maintainer-approved acceptance scope; retain known
   distribution limitations without adding unrelated feature or signing work.
2. Packaging-related PRs run the Release workflow with read-only permissions
   and ad-hoc macOS signing. Once the workflow exists on the default branch,
   a manual branch run can also build packages. Neither creates a release.
3. Require all 12 program assets plus `checksums.txt`: Windows Setup/ZIP,
   Linux AppImage/tar.gz, two macOS DMG/ZIP pairs, and the four Linux/macOS
   remote-server binaries. Inspect the packaged runtime and installation
   evidence, not only source-build results.
4. Download the `verified-release-assets` artifact to a fresh directory and run
   `python .github/scripts/verify-release-assets.py <directory> <version> --checksums`.
   Confirm the embedded versions, architectures, updater helpers, Windows
   ConPTY pair, and bundled WSL server. State signing/notarization precisely.
5. Create the matching version tag from the accepted commit after review. Tag
   runs may assemble an unpublished draft; they cannot overwrite an already published release.
6. Verify the draft's complete asset set again. Write the GitHub release notes
   in `docs/releases/hitty-v<version>.md` in English: a one-line summary (plus
   an upgrade note when users must act), then `Highlights`, `Changes`
   (`Added` / `Changed` / `Fixed` / `Internal`), `Known limitations`, and a
   `Full changelog` compare link to the previous Hitty tag. End each change with
   a qualified reference such as `cloudy-liu/hitty#34`; omit per-commit
   explanations, commit counts, and checksum detail. Keep logs and CI IDs in
   verification records. The first release records its source snapshot; later
   releases compare against the previous Hitty tag.
7. Publish explicitly only after the platform acceptance gate is complete.
   Then verify the latest-release endpoint and application/server downloads.
   After publication, fix defects in a subsequent version; do not move the
   published tag or replace its program files.

No legacy-product migration, compatibility aliases, or personalized default
configuration are part of Hitty's first release.

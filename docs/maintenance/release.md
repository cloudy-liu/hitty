# ctty7 release runbook

The first ctty7 release, v0.1.0, was published on 2026-09-29. Its migration
spec cloudy-liu/ctty7#48 and tickets #49–#52 are closed. The checklist below
retains the one-time migration steps; use the version and build sections for
subsequent releases. See [project status](status.md) for the published baseline
and unreleased work. Historical platform installation limits still apply.

## First-release migration checklist (historical)

1. Merge the reviewed migration PR into this fork. Use the current fork main;
   upstream synchronization and Markdown preview are outside this release.
2. Require successful Windows, macOS and Linux CI. Track an intermittent Windows
   hang in cloudy-liu/ctty7#45 if it recurs; preserve logs rather than hiding it
   behind unconditional retries.
3. Run the Release workflow manually on the candidate branch. Manual runs build
   the same platform packages but do not create a GitHub Release or tag.
4. Verify the old v26.8.3-c.9 to 0.1.0 migration on isolated installations.
   Check both Windows per-user and all-users scope, portable layout, old data,
   shortcut cleanup, icons and uninstall entries. Verify Mac bundle replacement
   and Linux AppImage replacement on their respective platforms. Confirm a
   subsequent official version can update. Record any unavailable checks.
5. Check CMD and Clink prompt editing/history, Herdr host application avatars
   with nested agents, agent restoration and WSL/SSH connection setup.
6. The GitHub repository was renamed from cloudy-liu/tty7 to
   cloudy-liu/ctty7 on 2026-09-29. Its fork relationship and history remain
   intact. The `fork` remote now uses
   `https://github.com/cloudy-liu/ctty7.git`; `origin` remains upstream.
   AGENTS.md and the issue-tracker configuration name the renamed fork and
   retain the prohibition on upstream PRs. Existing issue and PR numbers stay
   on this repository. Pages remains disabled.
7. Verify the new repository links and update/download endpoints resolve.
   Code and metadata target the new address; do not distribute the candidate
   before that address and its verified release are ready.

## Version and release notes

Use a strict `vX.Y.Z` tag matching the workspace version and package metadata.
The first ctty7 release is v0.1.0. Keep all previous tags and Releases.
There is one official release channel; no rolling or prerelease channel.

Before freezing the candidate, inspect local stashes and ctty7-related temporary
patches and worktrees. Compare their behavior against the candidate, because a
squash merge changes commit identities and an empty working tree does not prove
that locally validated changes were merged. Record which changes are included,
already present, superseded, or intentionally archived. Preserve the originals
until their disposition is established; do not clear local edits merely to make
release preparation clean. The [v0.7.1 audit](../testing/local-change-audit-v0.7.1.md)
records the default-HTML-preview omission that this check prevents.

At release preparation time, query the latest published custom release again
and review the full range from that tag to the intended release commit,
including merge and release commits, so no change is missed.

The GitHub Release notes and the release commit body are written in English
only, following the format in AGENTS.md:

- a one-line summary, then an upgrade note when the user must act, `Highlights`,
  `Changes` (Added / Changed / Fixed / Internal), `Known limitations` and a
  `Full changelog` compare link to the previous tag;
- one line per change, ending with a qualified issue or PR reference such as
  cloudy-liu/ctty7#46, with no per-commit explanation, commit counts or
  branch-line counts;
- patch-equivalent history syncs identified in one line under `Internal`;
- no inherited capabilities presented as new, and no CI run IDs or checksum
  detail; link the one-time manual migration guide when it applies.

Choose the next semantic version for the accepted scope. Query the repository's
latest published release instead of inferring it from this runbook's historical
verification records.

For the current 0.x release line, use a patch increment for corrections and
refinements to existing capabilities, including interaction and visual polish.
Use a minor increment for a distinct new user-facing capability or a broader
feature release. Choose from the accepted product scope, not the number of
merged PRs or their commit prefixes. For example, v0.8.0 introduced independent
document tabs and macOS HTML preview; v0.8.1 refines those existing workflows,
Mermaid reading, and attention indicators. A transition to 1.0 requires an
explicit decision about the stable product contract.

## Build, verify, publish

1. Push the matching version tag to the fork. The workflow builds and assembles
   a draft. A rerun may update that draft, but cannot overwrite a published
   release. Fix a failed candidate before publication.
2. Require all 12 program assets: eight GUI packages across Windows x86_64,
   Linux x86_64, macOS arm64 and x86_64; four Linux/macOS remote servers.
   Windows includes Setup and ZIP, Linux AppImage and tar.gz, and each Mac
   architecture DMG and ZIP. Internal server asset names remain `tty7-server-*`.
3. Verify package markers, executable versions and architecture, Windows
   ConPTY pairs and bundled WSL server, Mac signatures, AppImage metadata and
   updater helpers. Use the existing platform verification steps and logs.
4. Download all draft assets to a fresh directory. Run the complete asset-set
   verifier: `python .github/scripts/verify-release-assets.py <download-dir>
   <version> --checksums`, using the candidate version without its `v` prefix.
   It checks every entry against the downloaded bytes. The draft must contain
   exactly those 12 assets plus checksums.txt.
5. Add the finalized release notes. Keep the release a draft until every asset
   and checksum has passed. Then explicitly publish it and set it as Latest;
   numerical sorting must not leave a historical 26.x release selected.
6. Confirm `/releases/latest` points to the new tag and all update package URLs
   resolve. Add a migration notice to the former latest Release when users need
   a manual installation to change release lines. Do not edit historical
   changelog entries to make them describe ctty7 retroactively.

Until publication, rollback is a normal code revert. Once published, do not
move the tag or replace released binaries: issue the next patch release.

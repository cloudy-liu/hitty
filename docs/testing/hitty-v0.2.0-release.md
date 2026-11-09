# Hitty v0.2.0 release verification

The right-panel change was squash-merged from PR #21 as `2df4c05`; release
preparation was squash-merged from PR #22 as `6e04e75`. The annotated `v0.2.0`
tag points to `6e04e75a66eb6e94df41db6c0ec37076f7b97df6`.

Published on November 9, 2026 at 00:32:49 UTC (08:32:49 Asia/Shanghai).
GitHub's latest-release endpoint returned `v0.2.0`; all 13 public download
endpoints responded successfully. Uploaded asset sizes and SHA-256 values
matched the verified local files, and the public checksum manifest matched too.

## Accepted builds and packages

- CI run `42535574682` passed all 10 jobs at release candidate
  `a1783cc249227a3c4da98090fba5b16e503c27a0`.
- Release run `42535574781` passed the Windows x86_64, Linux x86_64, macOS ARM64
  and macOS Intel builds, all remote-server builds and complete asset checks.
- Five package runtime checks reported `hitty 0.2.0`, command input/output and
  GUI restart/session reconnect: Windows portable and installed, Linux tarball,
  and both macOS bundles. Windows install/uninstall also passed.
- Native Markdown checks passed on Linux and both macOS targets. Both macOS
  native HTML fixtures passed headings, stale targets, document/session scroll
  ownership, refresh and failure recovery. Representative Linux and Intel
  screenshots were inspected; the ARM64 runner still has a clipped desktop.
- All 12 program assets and `checksums.txt` were downloaded and rehashed locally.
  macOS bundle versions and executable architectures, Linux executable modes,
  Windows portable marker and PE versions, ConPTY pair, updater helpers and the
  bundled WSL server were checked. Linux tarballs remain manual-update packages;
  AppImage's helper and version marker passed CI verification.

## Windows v0.1.0 to v0.2.0 portable update

The unchanged `scripts/bootstrap-windows-update-smoke.py` used the actual
published v0.1.0 ZIP and the v0.2.0 release candidate in isolated directories.
It rejected the incorrect SHA-256, accepted the correct archive, replaced the
application, relaunched it and restored its workspace. Terminal output,
custom font size and an unmanaged user file were retained. Before/After native
images were inspected. The v0.2.0 ZIP SHA-256 is
`4b153b661dc18850f3c43d421284026650625318e63717d28984151bc41c958b`.

## Artifact promotion and limitations

The candidate and merged tag have identical Git trees. The exact assets from
run `42535574781` were promoted to a draft rather than rebuilt. Redundant tag
run `42540137015` was cancelled before its draft job; the duplicate merge CI
run `42540108191` was also cancelled after tree equality was confirmed.

Windows remains unsigned and both macOS bundles are ad-hoc signed without
notarization. Installer/macOS/Linux version-to-version update flows were not
newly exercised; Windows portable updating was. No external dependency version,
revision or checksum changed. The original v0.1.0 release remains immutable.

The publication timestamp, complete asset digests and public-download verification
are recorded in the companion `hitty-v0.2.0-release.json`.

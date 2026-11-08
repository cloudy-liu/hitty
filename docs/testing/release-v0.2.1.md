# v0.2.1 release verification

Published on 2026-10-09 at 00:55:23 UTC and explicitly set Latest.
[ctty7 v0.2.1](https://github.com/cloudy-liu/ctty7/releases/tag/v0.2.1)
contains 12 program assets and `checksums.txt`. Every asset was downloaded and
verified while the release was still a draft. Published tags and binaries
must not be replaced.

## Scope and source

- Repository: `cloudy-liu/ctty7`; frozen tag: `v0.2.1`, commit `16d52c9270039c3d22d79b30f0f965b5c226d539`.
- Previous release: v0.2.0, published on 2026-10-08 and superseded as Latest by v0.2.1.
- [PR #118](https://github.com/cloudy-liu/ctty7/pull/118) adds Skill metadata tables, CRLF Mermaid recognition, and larger code Copy controls.
- [PR #119](https://github.com/cloudy-liu/ctty7/pull/119) adds consistent inline and expanded Mermaid navigation controls.
- [Release preparation PR #120](https://github.com/cloudy-liu/ctty7/pull/120) updates all four workspace packages and lockfile entries to 0.2.1. Third-party dependencies are unchanged from the merged source baseline `bc807863`.
- The merged release tree exactly matches the locally tested candidate `ef202739`. The full range from v0.2.0 was reviewed; publication documentation adds no application behavior.
- The English release commit body, [repository notes](../releases/v0.2.1.md), and GitHub Release body match.

## Verification

| Check | Result |
| --- | --- |
| Workspace version and locked metadata | All four packages and their Cargo.lock entries report 0.2.1. |
| `cargo fmt --all -- --check` and `git diff --check` | Passed. |
| `cargo test --locked --workspace` | 2,992 passed, eight existing ignored tests, zero failures. Git for Windows' `usr/bin` utilities were on the test process's PATH. |
| `cargo test --locked --features updater --bin tty7-updater` | All 17 tests passed. |
| Release preparation CI | All nine jobs passed in [run 37863717276](https://github.com/cloudy-liu/ctty7/actions/runs/37863717276). |
| Frozen release source CI | All nine jobs passed in [run 37864073395](https://github.com/cloudy-liu/ctty7/actions/runs/37864073395), including Windows, Linux, macOS, standalone servers, and the native Markdown screenshot matrices. |
| Release workflow | All ten jobs passed in [run 37864082345](https://github.com/cloudy-liu/ctty7/actions/runs/37864082345), including platform packaging, Mac signing and architecture checks, Windows update layout, Linux AppImage metadata, and draft assembly. |
| Downloaded assets | The existing asset-set verifier passed for all 12 program assets and checksums. Every downloaded size and GitHub-reported SHA-256 digest also matched. |
| Package contents | Windows PE, Linux ELF, and Mac Mach-O architectures match their filenames. Both Mac ZIPs report bundle version 0.2.1 and contain the updater. The DMGs and AppImage have their expected formats. Windows ZIP and Linux tar.gz README contents match the frozen tag. |
| Windows package versions | GUI and updater ProductVersion are 0.2.1. Setup's numeric file version is 0.2.1.0 and its trimmed product version is 0.2.1. |
| Windows package layout | The portable marker and license notices are present. The ConPTY pair has matching FileVersion 1.24.2607.10001, and the bundled WSL server is byte-identical to the standalone asset. |
| Packaged executable versions | Windows `tty7 --version` and the downloaded Linux x64 server executed under Ubuntu WSL both report 0.2.1. The CLI has no PE version-resource fields. |
| Final draft and published metadata | Asset identities, sizes, and digests were unchanged through publication; the remote tag still resolved to the verified commit. The release is published, not a draft or prerelease, and explicitly Latest. |
| Public endpoints | `/releases/latest` redirects to v0.2.1 with HTTP 200; all 13 public asset URLs return HTTP 200. |

## Publication and evidence

The tag workflow assembled a draft. After source CI, packaging, downloaded-byte
checks, package inspection, and live asset rechecks passed, the English notes
were applied and the draft was explicitly published. Publication records are
maintained separately from the frozen release tag.

Local evidence is retained under the ignored
`.tty7-dev-release/v0.2.1-verification/` directory, including CI and release
metadata, draft and published asset snapshots, downloaded packages, version
checks, and public-endpoint results. Local test logs remain in
`.tty7-dev-release/v0.2.1-*.log`.

## Remaining acceptance limits

Native interaction acceptance for the new metadata tables and Mermaid controls
uses the recorded Windows checks in the [Skill preview](markdown-skill-preview.md)
and [Mermaid controls](markdown-mermaid-controls.md) records. Equivalent macOS
and Linux desktop interactions and in-place upgrades from v0.2.0 have not been
manually checked. The eight ignored workspace tests were not run. Earlier
remote-resource, near-limit scrolling, source-editor, diff, and Windows Cursor
acceptance gaps remain in the release notes. Mac packages are ad hoc signed
and not notarized.

# v0.3.0 release verification

Published on 2026-10-11 at 17:13:07 UTC and explicitly set Latest.
[ctty7 v0.3.0](https://github.com/cloudy-liu/ctty7/releases/tag/v0.3.0)
contains 12 program assets and `checksums.txt`. Every asset was downloaded and
verified while the release was still a draft. Published tags and binaries
must not be replaced.

## Scope and source

- Repository: `cloudy-liu/ctty7`; frozen tag: `v0.3.0`, commit `3fc65a9d6a58479ecb25239cc50c6a41c1d263c8`.
- Previous release: v0.2.2, published on 2026-10-10 and superseded as Latest by v0.3.0.
- PR #138 adds cross-platform agent sounds and independent desktop notification policies, whole-application focus checks, and lifecycle deduplication.
- PR #130 adds Copy Name; PR #128 restores agent Ctrl+V paste; PR #129 stabilizes Cursor identity across legacy hooks.
- PR #131 fixes Codex draft status and expanded Mermaid navigation; PR #132 protects daemon PID records until locks are released.
- Release preparation PR #139 updates all four workspace packages and lockfile entries to 0.3.0. Third-party dependencies are unchanged from source baseline `1d06832b`.
- The merged release tree exactly matches candidate `08c2988b`. The full range from v0.2.2 was reviewed, including the previous publication documentation.
- The English release commit body, [repository notes](../releases/v0.3.0.md), and GitHub Release body match.

## Verification

| Check | Result |
| --- | --- |
| Workspace version and locked metadata | All four packages and their Cargo.lock entries report 0.3.0; third-party entries are unchanged. |
| Local formatting and whitespace | `cargo fmt --all -- --check` and `git diff --check` passed. |
| Frozen source CI | All nine jobs passed in [run 38155517374](https://github.com/cloudy-liu/ctty7/actions/runs/38155517374), including Windows, Linux, macOS, standalone servers, updater tests, and native Markdown evidence. |
| Release workflow | All ten jobs passed in [run 38155524755](https://github.com/cloudy-liu/ctty7/actions/runs/38155524755), including packaging, Mac signature and architecture checks, Windows update layout, AppImage metadata, and draft assembly. |
| Downloaded asset set | All 12 program assets and checksums passed the existing verifier; all 13 downloaded sizes and GitHub-reported SHA-256 digests matched. |
| Package contents | Windows PE, Linux ELF, and Mac Mach-O binaries match their target architectures; both Mac ZIPs report bundle version 0.3.0 and contain updater and signature resources. DMGs and AppImage have their expected formats. |
| Packaged text | Windows and Linux README and license contents match the source after line-ending normalization. |
| Windows versions | GUI and updater ProductVersion are 0.3.0; Setup's numeric file version is 0.3.0.0 and its trimmed product version is 0.3.0. |
| Windows layout | Portable marker, completions, license notices, and matching vendored ConPTY files are present; the bundled WSL server equals its standalone asset. |
| Executable versions | The downloaded Windows CLI and the Linux x64 server executed under Ubuntu WSL both report 0.3.0. |
| Notification audio | The exact source WAV is embedded in Windows, both Mac architectures, the Linux tarball, and AppImage; the AppImage contains PulseAudio client libraries and updater helpers. |
| Publication | Asset identities, sizes, and digests were rechecked before and after publication; the remote tag resolves to the frozen commit and the release is published, not a draft or prerelease. |
| Public endpoints | `/releases/latest` redirects to v0.3.0; all 13 public asset URLs return HTTP 200. |

## Evidence and acceptance limits

Local evidence is retained under
`C:/Users/cloudy/.codex/task-notes/ctty7-v0.3.0/verification/`, including CI
metadata, draft and published asset snapshots, package and executable checks,
embedded audio inspection, and public endpoint results. Package copies and
extraction directories are removed after verification. The pre-release
cleanup preserved Git history and uncommitted source separately from generated
build and test files.

No full local rebuild was performed after the requested Cargo cleanup. The
frozen source CI provides the platform build and regression results. Actual
audible agent alerts and live notification behavior are not established by
package inspection or API success. Windows audio checks ran with system output
muted; native macOS/Linux audio acceptance remains unverified. Real
Setup/UAC and in-place upgrades from v0.2.2 were not manually checked. Mac
packages are ad hoc signed and not notarized. The
[release notes](../releases/v0.3.0.md) retain the other manual acceptance gaps.

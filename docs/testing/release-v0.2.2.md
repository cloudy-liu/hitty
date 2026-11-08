# v0.2.2 release verification

Published on 2026-10-10 at 00:58:40 UTC and explicitly set Latest.
[ctty7 v0.2.2](https://github.com/cloudy-liu/ctty7/releases/tag/v0.2.2)
contains 12 program assets and `checksums.txt`. Every asset was downloaded and
verified while the release was still a draft. Published tags and binaries
must not be replaced.

## Scope and source

- Repository: `cloudy-liu/ctty7`; frozen tag: `v0.2.2`, commit `cafd9ae588f1142f66b513a9c50f0a855c3f2dff`.
- Previous release: v0.2.1, published on 2026-10-09 and superseded as Latest by v0.2.2.
- PR #122 adds SSH corner badges and connection-target fallback to top tabs and sidebar rows; PR #123 changes sidebar status tags to lowercase with deeper colors.
- PR #124 adds system certificate trust and clears obsolete download errors; PR #125 adds update cancellation, process deadlines, proxy redirect handling, and recovery preservation.
- Release preparation PR #126 updates all four workspace packages and lockfile entries to 0.2.2. Third-party dependencies are unchanged from source baseline `320dee48`.
- The merged release tree exactly matches locally tested candidate `e90056c6`. The full range from v0.2.1 was reviewed, including the earlier publication documentation.
- The English release commit body, [repository notes](../releases/v0.2.2.md), and GitHub Release body match.

## Verification

| Check | Result |
| --- | --- |
| Workspace version and locked metadata | All four packages and their Cargo.lock entries report 0.2.2. |
| Formatting and whitespace | `cargo fmt --all -- --check` and `git diff --check` passed. |
| Local workspace suite | `cargo test --locked --workspace` passed 3,019 tests with ten ignored fixture or opt-in tests and zero failures. Git for Windows utilities were on the test process's PATH. |
| Local updater suite | All 32 tests passed; one child-process fixture is ignored at the top level and invoked by its surrounding tests. |
| Frozen source CI | All nine jobs passed in [run 38008120429](https://github.com/cloudy-liu/ctty7/actions/runs/38008120429), including Windows, Linux, macOS, standalone servers, and native Markdown and SSH tab evidence. |
| Initial candidate failure | The candidate's macOS server integration suite failed the existing `reap_stranded_clears_a_seat_holder_with_no_pidfile` assertion. That test and its daemon spawn path are unchanged since v0.2.1. The exact test and all macOS suites passed in the frozen source CI; the original failure log is retained. |
| Release workflow | All ten jobs passed in [run 38008127254](https://github.com/cloudy-liu/ctty7/actions/runs/38008127254), including platform packaging, Mac signing and architecture checks, Windows update layout, AppImage metadata, and draft assembly. |
| Downloaded asset set | The existing asset-set verifier passed for all 12 program assets and checksums. All 13 downloaded sizes and GitHub-reported SHA-256 digests matched. |
| Package contents | Windows portable binaries, Linux ELF binaries, and Mac Mach-O binaries match their target architectures. Both Mac ZIPs report bundle version 0.2.2 and contain the updater and signature resources. DMGs and AppImage have the expected formats. |
| Packaged text | Windows README and license files use CRLF while the local source uses LF. Their contents match after line-ending normalization; Linux archive text also matches the source. |
| Windows versions | GUI and updater ProductVersion are 0.2.2. Setup's numeric file version is 0.2.2.0 and its trimmed product version is 0.2.2. |
| Windows layout | The portable marker, completions, license notices, and ConPTY pair are present. ConPTY files match the vendored source and both report 1.24.2607.10001. The bundled WSL server equals the standalone asset. |
| Executable versions | The downloaded Windows CLI and the Linux x64 server executed under Ubuntu WSL both report 0.2.2. |
| Publication | Asset identities, sizes, and digests were rechecked before and after publishing. The tag still resolves to the frozen source. The release is published, not a draft or prerelease, and explicitly Latest. |
| Public endpoints | `/releases/latest` resolves to v0.2.2 with HTTP 200; all 13 public asset URLs return HTTP 200. |

## Evidence and acceptance limits

Local evidence is retained under the ignored
`.tty7-dev-release/v0.2.2-verification/` directory, including CI metadata,
draft and published asset snapshots, downloaded size and digest records,
package inspection, executable versions, and public endpoint results.
Local test and failure logs remain in `.tty7-dev-release/v0.2.2-*.log`.
These records remain available after build products and package copies are
cleaned.

The affected computer's certificate environment, real Setup/UAC upgrades,
and in-place upgrades from v0.2.1 were not manually checked. Native badge
evidence uses deterministic Windows and Linux fixtures, not live remote-agent
detection. Remaining appearance and interaction acceptance gaps are listed in
the [release notes](../releases/v0.2.2.md). Mac packages are ad hoc signed and
not notarized. Windows Setup has no complete application-owned rollback.

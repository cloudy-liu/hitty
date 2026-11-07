# v0.2.0 release verification

Prepared on 2026-10-07 and published on 2026-10-08 at 00:44:59 UTC.
[ctty7 v0.2.0](https://github.com/cloudy-liu/ctty7/releases/tag/v0.2.0) was set
Latest at publication and was superseded by v0.2.1 on 2026-10-09.
The complete asset set was downloaded and verified while the release was still
a draft. Published tags and binaries must not be replaced.

## Scope and tracker state

- Repository: `cloudy-liu/ctty7`; published tag: `v0.2.0`, commit `f9416a5232b675e38d2899b668ef301454275c89`.
- Refreshed starting commit: `8fcc511a49160ee5278695c11723565990790480`, matching fetched fork main on 2026-10-08. The original preparation used `b4f7b086`.
- Previous release: v0.1.0, published 2026-09-29 and superseded as Latest by v0.2.0.
- Release preparation [PR #115](https://github.com/cloudy-liu/ctty7/pull/115) and final bilingual README [PR #116](https://github.com/cloudy-liu/ctty7/pull/116) are merged.
- PR #103 implements #100; both the parent and child #101 are closed. PR #115 merged the native checks and documentation in the [acceptance record](sidebar-status-tags.md).
- The full history since v0.1.0 was reviewed, including the baseline merge and patch-equivalent terminal-color history reconciliation.
- PRs #112, #113, and #114 add diff width hardening, stable Cursor hook/session identity, and a coherent GitHub diff palette. Their changes and remaining acceptance gaps are included in the release notes.

## Local changes

- Workspace version and the four workspace packages in Cargo.lock now use 0.2.0. Third-party dependency versions are unchanged.
- [English README](../../README.md) and [Chinese README](../../README.zh-CN.md) explain project identity, persistence, fork-specific workflows, installation, and building. PR #116 includes the user's final rewrite and removes the publication notice.
- One combined light/dark agent-terminal image replaces the inherited README hero, alongside a separate source-editor screenshot. [Capture details](../../assets/screenshots/README.md) identify the real CLIs, sample sidebar hook events, and source build.
- Both READMEs retain the original pink typing-title animation. The logo and title share a centered paragraph with line breaks and image-margin overrides for preview compatibility.
- [English release notes](../releases/v0.2.0.md) follow the previous GitHub Release structure and the current AGENTS.md rules.
- The CLI endpoint-resolution test reuses its existing platform shell helper instead of requiring `sh` on Windows. Its expected exit code and endpoint assertions are unchanged.
- Status documentation now covers uppercase tags, the status-text option, and Codex's ready prompt.
- The cached Markdown view now fills the viewport, observes layout without adding scrollable blank space, and clamps the scroll offset when a document shrinks.
- The built-in GitHub dark reading theme follows the application's neutral foreground while preserving semantic colors and custom-theme palettes.

## Verification

The format, workspace check, complete workspace test suite, desktop updater
tests, and version-metadata checks were rerun successfully on 2026-10-08 with
all changes through `8fcc511a` and this release preparation. Four workspace
packages report 0.2.0; third-party dependency versions are unchanged. Logs are
retained in the ignored `.tty7-dev-release/release-v0.2.0-*.log` files.

The native app and README image, preview, and header checks below describe the
original local review. Final README text and links were checked after PR #116;
the screenshot assets and header markup did not change in that rewrite.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `cargo check --locked --workspace` | Passed. |
| `cargo test --locked --workspace` | Passed, with eight existing ignored tests. |
| `cargo test --locked --features updater --bin tty7-updater` | All 17 tests passed. |
| `git diff --check` | Passed. |
| Markdown scroll and dark foreground regressions | Passed as part of the full workspace suite, including dock/fill bounds, the final block, short-document clamping, live theme changes, Rust neutral tokens, and custom-theme preservation. |
| Native app | The linked executable reports file version 0.2.0; sidebar, Markdown/Mermaid, source/preview switching, and light/dark appearance were exercised. |
| README assets | The combined light/dark hero preserves both native screenshot regions pixel for pixel; Claude Code's orange mascot was captured after removing inherited NO_COLOR from the isolated screenshot processes. The source-editor WebP also reproduces its original capture. |
| Initial README preview | The reviewed Chinese preview loaded the combined agent hero, logo, and source-editor screenshot without page overflow at 372 pixels. Final READMEs' local image and document paths were checked again. |
| README header alignment and spacing | Typora's Chinese preview displays the logo and animated title on the same vertical centerline with reduced spacing. The browser preview measures a 29.7 CSS-pixel gap, matching the current GitHub README; both images share a center with no page overflow at 372 pixels. |
| Starting main CI | All nine jobs passed in [run 37500111063](https://github.com/cloudy-liu/ctty7/actions/runs/37500111063). This was the original source baseline before release preparation. |
| Refreshed main CI | All nine jobs passed in [run 37644303956](https://github.com/cloudy-liu/ctty7/actions/runs/37644303956) on `8fcc511a`. Release PR CI is tracked separately. |
| Final published source CI | All nine jobs passed in [run 37706632693](https://github.com/cloudy-liu/ctty7/actions/runs/37706632693) on `f9416a52`. |
| Final Release workflow | All ten jobs passed in [run 37706707010](https://github.com/cloudy-liu/ctty7/actions/runs/37706707010), including platform packaging, Mac signing/architecture checks, Windows update layout, Linux AppImage metadata, and draft assembly. |
| Downloaded assets | The existing asset-set verifier passed for all 12 program assets and `checksums.txt`. File sizes and all GitHub-reported SHA-256 digests also matched the downloaded bytes. |
| Package contents | Windows PE, Linux ELF, and Mac Mach-O architectures match their filenames. Both Mac ZIPs report bundle version 0.2.0 and include the updater; the DMGs and AppImage have their expected formats. Windows ZIP and Linux tar.gz README contents match the final tag. |
| Windows release | GUI and updater report ProductVersion 0.2.0; Setup reports 0.2.0; the ConPTY pair reports matching FileVersion 1.24.2607.10001. The portable marker and MIT notice are present, and the bundled WSL server is byte-identical to the standalone asset. |
| Packaged executable versions | Windows `tty7 --version` and the downloaded Linux server executed under Ubuntu WSL both report 0.2.0. |
| Published endpoints | `/releases/latest` redirects to v0.2.0 with HTTP 200; all 13 asset download URLs return HTTP 200. |

The Windows full suite needs Git for Windows' `usr/bin` utilities in its process
PATH. The first run exposed the CLI test's hardcoded `sh`; later core tests also
needed the existing `cat` and `sleep` utilities. The passing run used that PATH.

`cargo build --locked --workspace` compiled and linked the new binaries, but its
final copy to `target/debug/tty7-app.exe` was blocked by an existing development
daemon hosting two live panes. That daemon was left running. The newly linked
`target/debug/deps/tty7_app.exe` was copied into the isolated review runtime for
native checks. This is not a packaged release build.

## Completed publication

1. The user authorized publication and the final README rewrite. Both preparation PRs targeted the fork's `main`; no upstream PR was created.
2. The first draft was built from `f0c601a9`. Before publication, the candidate tag was updated to `f9416a52` to include PR #116. Only the two READMEs changed; code, dependencies, and version remained identical.
3. The final source CI and Release workflow passed. Every draft asset was freshly uploaded by the final build and downloaded into a separate verification directory.
4. `python .github/scripts/verify-release-assets.py <directory> 0.2.0 --checksums` passed. Additional package inspection checked versions, architecture, final README contents, ConPTY, and the bundled WSL server.
5. Asset IDs, sizes, and digests were rechecked against the live draft before publication, and the remote tag still resolved to the verified commit.
6. The English notes were published with the release explicitly marked Latest. The published release identity, complete asset list, latest redirect, and all public download endpoints were verified.

Local evidence is retained under the ignored
`.tty7-dev-release/v0.2.0-final-verification/` directory, including source CI,
release-job metadata, release logs, downloaded assets, and published metadata.

In-place upgrades from v0.1.0 remain manually unverified on Windows, macOS, and
Linux. Mac packages are ad hoc signed and not notarized. The other outstanding
native and remote-resource checks remain in the release notes; package checks
do not establish those acceptance results.

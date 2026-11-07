# Open issue audit on 2026-10-23

Checked the three open issues against fork main `b0b57316`, their merged PRs,
the implementation, and the native artifacts from
[main CI](https://github.com/cloudy-liu/ctty7/actions/runs/39921343329).
The follow-up also checked all ten successful jobs and both macOS HTML
artifacts from [PR #193 CI](https://github.com/cloudy-liu/ctty7/actions/runs/39940468612),
then continued native Windows acceptance for the taskbar.

## Disposition

| Issue | Implementation already on main | Remaining work |
| --- | --- | --- |
| [#182](https://github.com/cloudy-liu/ctty7/issues/182), multi-file document tabs | [#186](https://github.com/cloudy-liu/ctty7/pull/186) closes child issues #183, #184 and #185, but only references the parent spec. [#191](https://github.com/cloudy-liu/ctty7/pull/191) adds HTML reading-position preservation. | This follow-up fixes the same-document fragment case found during the audit and links the parent for closure when the PR merges. |
| [#175](https://github.com/cloudy-liu/ctty7/issues/175), native HTML preview | [#178](https://github.com/cloudy-liu/ctty7/pull/178) implements WKWebView and shared document behavior. Windows preview already exists; Linux integration boundaries are documented. The original PR used `Refs`, so its merge did not close the spec. | Closed as the delivered implementation at the user's request. PR #178 now includes its closing reference. Unverified device/performance/package cases remain documented limits, not claimed passes. |
| [#188](https://github.com/cloudy-liu/ctty7/issues/188), upper-right taskbar attention | [#189](https://github.com/cloudy-liu/ctty7/pull/189) composes attention into both window icon slots. | Continued native acceptance found a stale taskbar bitmap after attention cleared. [#194](https://github.com/cloudy-liu/ctty7/pull/194) fixes class-icon restoration, includes real Before/After captures, and closes this issue when merged. |

The open issue count therefore does not describe three missing implementations.
The document spec had an uncovered HTML edge case, and continued taskbar
acceptance found a separate clearing bug. Both have focused fix PRs. The
completed platform implementation is closed with its verification limits
recorded. Issue titles/headings use English and explanatory prose uses Chinese.

## HTML fragment regression

Following a page link changes `file:///.../report.HTML` to
`file:///.../report.HTML#reading-position`. Scroll capture previously compared
the entire browser URL with the saved file URL, so it discarded a valid
600-CSS-pixel position. Returning from another HTML tab restored the top instead.
The restoration guard also compared the entire URL, affecting refresh after
fragment navigation.

Ignore the fragment in both browser-owned URL checks, while retaining the
file path and query. Encoded `#` characters in filenames remain `%23` and are
part of the file identity. The existing native fixture follows an actual
same-document link, switches files, refreshes the fragment-bearing page and
checks the observable viewport and continued preview availability. Wait for
refresh restoration before starting the next interaction so the test does not
race an earlier restore.

Reproduction uses the production document opening and preview actions:

```powershell
cargo build --locked --features markdown-visual-tests --bin tty7-app
python scripts/html-native-preview.py target/debug/tty7-app.exe OUTPUT_DIRECTORY
```

The native regression was observed red in two independent stages:

- Original URL checks: `HTML reading position was lost across file tabs`.
  The browser returned 600 CSS pixels, but the fragment-bearing URL caused
  the document panel to discard the snapshot.
- Capture normalized, restoration unchanged: the file switch restored 600,
  but refreshing the fragment-bearing page failed with
  `HTML reading position 0 was not restored` and fell back to source.

With both guards normalized, the complete Windows acceptance passes and
reports `fragment_scroll_and_refresh: true`. It also passes existing resource,
session ownership, browser recreation, rapid-switch, close/reopen and failure
recovery checks. The input filename is now `report #1.HTML`, verifying `%23`
in the file path alongside Unicode, spaces and an uppercase extension. This
run used Windows 11 build 22000.2538, WebView2 155 and DPR 1.75. These checks
establish behavior, not a performance improvement.

Final local checks pass: `cargo test --workspace --locked` reports 3,157 passed
and 12 ignored across its standard test summaries; the native visual-feature
build, `cargo fmt --all -- --check`, `git diff --check` and the Host boundary
check also pass. Temporary diagnostic logging was removed before the passing
native run.

## Existing macOS evidence inspected

Downloaded and inspected both `native-html-aarch64-apple-darwin` and
`native-html-x86_64-apple-darwin` artifacts from the main and PR CI runs above.
Both report successful relative CSS/JavaScript/SVG loading, script interaction,
refresh, singleton reuse, per-file/session scrolling, browser recreation,
rapid selection, close/reopen and deleted-file source recovery. The native
desktop images visibly show the HTML child view, loaded resources, file tabs,
breadcrumbs and Preview/HTML controls on both architectures.

Both PR artifacts additionally report `fragment_scroll_and_refresh: true`,
including the literal `#` filename. DOM script clicks do not establish native mouse/keyboard
focus behavior. The single polling-based timing samples do not establish
first visible paint, release-profile performance or comparative resource use.
The native input/layout/occlusion/DPI matrix, comparative process-group resource
measurements and packaged-runtime checks remain verification limits. Closing
the delivered platform spec does not mark these checks as passed.

## Taskbar evidence

The initial ten tray tests passed, but continued real Explorer acceptance
found a missing case: restoring a null window override did not clear the
taskbar bitmap when GPUI used a class icon. The state fixture reported aggregate
`attention: false` with all panes read while the actual taskbar still had a dot.

PR #194 preserves the borrowed class icon instead. Its new real Win32-class
regression failed before the fix; all eleven tray tests and the workspace suite
pass afterwards. Native lossless Before/After crops show the stale dot and its
removal under the same cleared-attention state. See its
[evidence](https://github.com/cloudy-liu/ctty7/pull/194).

The horizontal Windows 11 taskbar is now captured. Vertical taskbars, live
orientation changes, other taskbar customizations and Explorer recreation
remain unverified. The older overlay screenshots retain their historical labels.

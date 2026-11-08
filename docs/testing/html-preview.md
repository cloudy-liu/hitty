# HTML preview validation

## Cross-platform acceptance

The document panel enables local HTML/HTM preview on Windows with WebView2 and
on macOS with WKWebView. The existing default-open, source-navigation,
dirty-buffer, mode-preservation and source-fallback tests run on both platforms.
The column-only navigation regression was observed failing before the fix and
passing afterwards. Test paths use the current platform's temporary directory.

CI builds and tests both `aarch64-apple-darwin` on macos-14 and
`x86_64-apple-darwin` on macos-15-intel, alongside Windows and Linux. Each Mac
also runs the real document panel via:

```sh
cargo build --locked --features markdown-visual-tests --bin tty7-app
python3 scripts/html-native-preview.py target/debug/tty7-app native-html
```

On Windows use `target/debug/tty7-app.exe`. The runner uses an isolated config
and copies the existing fixture into a directory containing Chinese characters
and spaces, with an uppercase `.HTML` extension. It opens the document through
the production editor entry point and uses actual WebView evaluation callbacks
to check relative JavaScript, SVG decoding, CSS grid, page dimensions, a DOM
button click and refresh. Arbitrary inspection exists only with
`markdown-visual-tests`; production builds evaluate only host-owned scroll
capture and restoration scripts and have no page-to-app script bridge.

The runner also switches to `next.htm` and back and checks that the same browser
instance serves both files. Resources live in a parent directory, exercising
`../assets/` as well as Unicode paths. It then opens a fixture-owned temporary
HTML file, deletes that file and refreshes, requiring asynchronous navigation
failure to restore source mode with the original buffer intact. WebView2
navigation-completed/process-failed events and a WKWebView navigation-delegate
observer feed the same document-panel recovery. Failures retain their request
identity, and stale requests or different active documents are ignored.

The file-tab regression scrolls the original page to 600 CSS pixels, switches
to a second HTML file and returns, requiring the same reading position. It also
requires the browser to release when switching to Markdown or source and to
restore the reading position after recreation. Separate sessions opening the
same path retain different positions; rapid selection changes and closing then
reopening a path must not transfer a stale position. Assertions allow less than
one CSS pixel for device-pixel rounding. The fixture page has sufficient height
for these checks at either viewport.
The browser captures the outgoing viewport before navigation and restores it
after the native load-completed event. File buffer identities keep independent
terminal sessions separate, including when they open the same path. Leaving
HTML preview releases the singleton after capture. The three-second evaluation
deadline recovers from a hung renderer; ordinary transitions wait for native
callbacks and do not use layout delays. Refresh starts the saved page at the top.

On 2026-10-23 the Windows native acceptance reproduced lost scroll after browser
recreation. The hidden gpui-wry child skipped layout, so restoration ran against
a zero-size viewport. Sizing the hidden viewport before restoration passed the
same acceptance, including independent sessions, rapid switches, close/reopen,
browser release and deleted-file recovery at 175% display scale.

The macOS artifact includes a desktop screenshot taken with `screencapture`.
GPUI's GPU-only capture cannot prove a native child view is visible. Inspect
the screenshot separately from the DOM results; successful evaluation or a
clean process exit does not establish visible rendering or native mouse input.
The artifact also records the browser user agent, architecture, DPR, viewport
and one cold resource-readiness sample. This sample includes document opening
and polling overhead and does not measure first visible paint or establish a
performance target.

On 2026-10-21 the Windows development run passed resources, CSS grid, script
interaction, refresh, both file switches, instance reuse and deleted-file
recovery. At a 489 × 829 CSS-pixel viewport and DPR 1.75 with Edge/WebView2 155,
one debug sample recorded 3,454.58 ms to polled resource readiness, 336.29 ms
for refresh and 378.80 / 395.56 ms for the two file switches. These are single
observations, including polling overhead, not percentiles or release benchmarks.

The same run used Windows 11 build 22000 and a Ryzen 7 5800H with 16 logical
processors. Three process-tree samples per state, using the existing
`scripts/bench/html-preview-memory.ps1`, gave these medians. The GUI and its
browser descendants are included; the shared session daemon is excluded.

| State | WebView2 processes | Private resident MiB | Private committed MiB | Idle CPU, one core % |
| --- | ---: | ---: | ---: | ---: |
| Active preview after refresh and switching | 6 | 133.12 | 329.25 | 0 |
| Source restored after navigation failure | 0 | 19.80 | 108.21 | 0 |

The source samples observed 5, then 0, then 0 browser processes as shutdown
completed. This establishes short-run process cleanup on this Windows sample,
not long-term leak behavior or macOS resource reclamation. There was no matched
pre-opening baseline or release-profile comparison in this run.

Final local `cargo test --workspace --locked` passed 3,131 tests with 12 ignored.
All six focused HTML regressions pass, including the two observed red/green
cases for column-only navigation and pending unsaved edits. `cargo check`,
the native visual-feature build, formatting, Host boundary and four pinned
agent-manifest Python tests also pass. Standards review found no violations;
Spec review found no remaining definite implementation blocker and retains
the native-device validation gap below.

Complete the following real-device checks before release, recording Windows,
Apple Silicon and Intel separately:

- Click and type with native input; move focus between the page, source,
  file tree and terminal, then verify ordinary terminal typing.
- Resize the window and document divider; switch dock/fill layouts and DPI.
- Open application menus, settings and dialogs; verify occlusion and restoration.
- Save source edits, preview again, refresh and open the original file externally.
- Switch repeatedly between HTML/HTM files; measure one reused browser instance,
  cold opening, switching and refresh, including browser load-finished events.
- Measure browser subprocess resident/private memory and idle CPU before preview,
  during repeated navigation, after leaving preview and after closing the window.
- Exercise initialization/runtime/navigation failures and confirm notifications,
  source recovery and buffer preservation.
- Start actual packaged applications and confirm local resources and runtime
  dependencies. Build-only checks do not establish packaged-runtime support.

This development machine runs Windows. macOS native results must come from
the architecture-specific CI artifacts or actual Macs. Missing-runtime machines,
the full native input/occlusion/DPI matrix, comparative resource benchmarks and
release-package execution remain separately recorded validation work. The
historical numbers below must not be presented as measurements of this change.

Linux CI retains source editing and the existing terminal/Markdown checks.
`cargo tree --locked --target x86_64-unknown-linux-gnu -i lb-wry` should report
no selected package. Linux X11, XWayland and native Wayland embedding are not
implemented; [the support boundaries](../specs/html-preview.md) describe their
different conditional paths.

## Historical Windows baseline

The Windows preview loads the original saved `file://` URL through WebView2.
Chrome is the browser reference. The application uses one lazy browser view per
window and releases it when the user leaves HTML preview.

## Default opening regression

Starting with v0.7.1, a newly opened saved local Windows HTML/HTM file requests
browser preview immediately. The render loop creates the browser lazily even
when no browser instance exists yet. Explicit line/column navigation opens the
source editor, and reactivating an already open file preserves its selected mode.
Unsaved source changes still require saving before preview or refresh.

The v0.7.0 release included the browser implementation from PR #172 but omitted
the local default-opening changes. Its original test explicitly required source
mode on first open. The preserved changes were recovered from the release
preparation stash; see the [local-change audit](local-change-audit-v0.7.1.md).
Regression coverage now checks both the selected mode and the render-loop sync
request, including before the first browser exists and after switching to source.

The browser comparisons and resource measurements below were recorded for
PR #172. They are historical measurements, not new v0.7.1 performance results.

## Visual evidence

| Before, source only | After, embedded browser |
| --- | --- |
| ![Before](images/html-preview/before.jpg) | ![After](images/html-preview/after.jpg) |

| Chrome through Chrome DevTools MCP | Embedded WebView2 through CDP |
| --- | --- |
| ![Chrome](images/html-preview/chrome-reference.png) | ![WebView2](images/html-preview/webview-reference.png) |

Both browsers load `tests/fixtures/html-preview/report.html` at 961 x 835 CSS
pixels, DPR 1.75, light color scheme and 100% zoom. The fixture uses relative CSS,
JavaScript and SVG files without network resources. Twelve selected elements
have equal bounding rectangles, computed fonts, colors, backgrounds and display
properties. Both report `All loaded` and a successfully decoded SVG image.
Raw measurements accompany the PNGs as `chrome-reference.json` and
`webview-reference.json`.

The pixel comparison uses the common image area without resizing. At this DPR,
Chrome exports 1682 x 1461 pixels and WebView2 exports 1682 x 1462. The final
bottom row is excluded from the comparison. `pixels.json` records the exact
comparison result. This fixture validates browser fidelity; it does not promise
identical rendering for every Chrome/WebView2 version, extension, font or site.

The final capture has 2,631 differing pixels out of 2,457,402, or 0.1071%.
Mean absolute RGB channel error is 0.00168 on a 0..255 scale.

## Windows composition

The previous GPUI target was above native child windows, hiding the webpage
even though its resources and JavaScript loaded. The failure is captured in
[the blank-preview screenshot](images/html-preview/direct-composition-blank.jpg).
Disabling DirectComposition made it visible but removed per-pixel alpha, so
that workaround was removed.

The final dependency commit changes `CreateTargetForHwnd(hwnd, true)` to
`CreateTargetForHwnd(hwnd, false)` and retains the existing composition swap
chain, GPU acceleration and alpha configuration. Microsoft's
[composition target documentation](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createtargetforhwnd)
defines this ordering relative to child windows. The change is one commit above
the previous locked dependency:
[GPUI child-window ordering fix](https://github.com/cloudy-liu/zed/commit/7a54867bb4b5b3766855d40f5cbd70d85f229d21).
All replacement GPUI packages use the same immutable source revision.

The terminal remains transparent at 50% opacity with the same controlled Chrome
fixture behind it. These screenshots verify that the previous alpha behavior
survives the composition-order change:

| Baseline at 50% opacity | Final dependency at 50% opacity |
| --- | --- |
| ![Before opacity](images/html-preview/before-opacity.jpg) | ![After opacity](images/html-preview/after-opacity.jpg) |

Native mouse input increments the fixture counter from 0 to 1. The application
menu hides the native child and displays correctly; Escape restores the page.
See [menu evidence](images/html-preview/menu.jpg).
Dragging the file-panel divider resizes the native child and reflows the page,
as shown in [the narrower preview](images/html-preview/resized.jpg). Refresh
resets the JavaScript click count from 1 to 0. File-tree search remains usable
while the page stays visible.

## Measurement method

Measurements were taken on Windows 11 build 22000, an AMD Ryzen 7 5800H with
16 logical processors and approximately 13.86 GiB of RAM. The application builds
use the debug profile with `markdown-visual-tests`, which enables load-time
logging. This is a development-machine sample, not a release-build benchmark.
The baseline is fork/main `f16436fa5e7ca76f3a4f0e3fe1dd018568cf03d7`.
Both builds use the same isolated configuration and restored terminal sessions.
The existing isolated daemon is shared between runs and excluded from both GUI
process groups.

`scripts/bench/html-preview-memory.ps1` takes five process-tree samples about
two seconds apart. It includes the GUI, console helper and descendant WebView2
processes. Private working set measures resident private pages; private commit
measures committed private memory. Summed total working set also includes shared
pages more than once and must not be interpreted as physical RAM consumption.
CPU is process CPU-time delta divided by elapsed time, with 100% meaning one
logical core. The initial sample has no CPU delta. Tables report medians.

| State | WebView2 processes | Private working set MiB | Summed working set MiB | Private commit MiB | Idle CPU, one core % |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline, HTML source | 0 | 37.42 | 127.71 | 190.39 | 0 |
| Final, HTML source | 0 | 37.03 | 119.64 | 190.28 | 0 |
| Final, active preview | 6 | 163.62 | 481.61 | 404.28 | 0 |
| Final, refresh and two navigations | 6 | 168.23 | 496.44 | 421.71 | 0 |
| Final, source restored | 0 | 37.40 | 125.50 | 190.62 | 0 |

Opening this fixture adds 126.59 MiB of private resident memory and 214.00 MiB
of private commit relative to final source mode. After refresh and navigation
from `report.html` to `next.htm` and back, the same browser process group remains
at six WebView2 processes. This short run does not establish long-term leak
behavior. Closing preview removes all six processes and leaves 0.37 MiB more
private resident memory and 0.34 MiB more private commit than source mode.
The small baseline/source difference is within run-to-run variation.

The first preview in the process, with an existing browser profile, takes
2,555.88 ms to the load-finished event. Two refreshes take 517.28 and 507.77 ms;
navigation to `next.htm` takes 451.96 ms and back to `report.html` takes 580.91 ms.
These five timings are individual observations, not percentile estimates.
No CDP port is enabled for these memory, CPU and load-time measurements.
Chrome and WebView2 both report Chromium major version 155 in the capture data.

Raw samples are in `perf/html-preview/`. Preview creation, refresh and navigation
times are wall-clock intervals ending at WRY's page-load-finished event; they
are not first-contentful-paint timings.

## Checks

`cargo build --locked --features markdown-visual-tests --bin tty7-app` passes.
`cargo test --workspace --locked` passes with 3,103 tests passed and 12 ignored,
including the new saved-file/dirty-buffer guard and preview-release tests and
the existing native Markdown tests. `cargo fmt --all --check` and the host
boundary script pass. Agent-detection manifest verification and its four Python
tests pass. The desktop updater suite passes with 32 passed and one ignored.
Standards and Spec reviews found no remaining source blockers after
the focus, target-size and composition changes.

## Reproduction

1. Build with `cargo build --locked --features markdown-visual-tests --bin tty7-app`.
2. Launch an isolated app configuration, open the fixture from the file tree,
   switch to source, and measure source mode with `html-preview-memory.ps1`.
3. Click the preview icon, exercise the page and refresh, then take active and
   closed process-tree samples using the same root PID.
4. For test captures only, launch with
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=49791` and run
   `node scripts/bench/html-preview-cdp.mjs http://127.0.0.1:49791 OUTPUT_PREFIX`.
5. Open the same fixture in Chrome through Chrome DevTools MCP, emulate the
   recorded viewport/DPR, evaluate `html-preview-metrics.js`, and capture a PNG.
6. Compare the PNGs with `html-preview-pixels.ps1`.

The production application does not enable a CDP port or install a JavaScript
bridge. HTML scripts execute in the browser environment. Pop-up windows are
denied; external opening uses the original saved URL and the OS browser setting.

## Scope and limits

The historical Windows baseline supported saved local Windows `.html`/`.htm` files. Unsaved changes
must be saved before preview or refresh. Remote Host files stay in the source
editor. macOS and Linux browser embedding were not enabled in that baseline. WebView2 Runtime is
required, and initialization errors return a notification with source editing
available. A genuinely missing-runtime machine was not available for testing.

The browser applies normal local-file origin restrictions, including restrictions
on modules and fetches. A page requiring a development server still requires
that server. Native menus/dialogs hide the child view while they are active.
Long terminal-output workloads, multi-monitor DPI transitions and a full
Mica/Acrylic compatibility matrix were not benchmarked.

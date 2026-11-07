# HTML preview

## User-visible contract

Local Windows and macOS `.html` and `.htm` files open in embedded browser preview by
default and can switch to the existing source editor. Explicit line navigation
opens source, including column-only links, and reactivating an open file keeps its selected mode. Preserve
the original document URL, CSS, JavaScript, relative resources and page colors.
Compare against the platform browser at the same viewport, zoom and resource-loading conditions.
WebView2, WKWebView and system fonts can produce different pixels; feature and
operation parity is the compatibility target.

The first version previews saved files, offers refresh and external browser
opening, and leaves unsaved edits intact. An unsaved buffer must ask the user
to save before previewing or refreshing, without silently saving it. Remote
Host files retain source editing and are outside this first version.

## Resource and platform boundaries

Create the system WebView only when preview is requested. Reuse at most one active HTML
preview per application window. Switching HTML files must not retain one
browser view per document. Closing preview releases the browser view; switching
to another application surface must not leave a native view covering it.

Use the gpui-wry package from the existing gpui-component revision where
possible. Windows requires Microsoft Edge WebView2 Runtime; macOS uses the system
WKWebView on both Apple Silicon and Intel. No Chromium runtime is bundled.
WKWebView loads the original file URL with local-volume read access so relative
resources, including `../` references, can resolve. Missing runtime
or view creation errors must be visible and recoverable in the source editor.
Do not add a JavaScript bridge to terminal or filesystem operations. Application
dialogs and menus must remain usable above the preview.

Linux retains source editing and does not link WRY, GTK or WebKitGTK through
this feature. Its potential integration paths are distinct:

| Environment | Underlying path | ctty7 status |
| --- | --- | --- |
| X11 | WRY supports native child windows; GTK initialization and event-loop integration are still needed. | Not implemented or validated with GPUI. |
| XWayland on a Wayland desktop | May use the X11 path when ctty7 itself runs its X11 backend. | Future compatibility path; not native Wayland support. |
| Native Wayland | WRY offers a GTK container approach. | The current GPUI window and adapter have no ready integration; requires separate design and validation. |

See the [WRY platform notes](https://github.com/tauri-apps/wry#child-webviews)
and [approved cross-platform spec](https://github.com/cloudy-liu/ctty7/issues/175).
None of these Linux paths is delivered by this change. Remote Host files also
stay in source mode; a remote path must never load a same-named local file.

Preview and refresh require a saved buffer. Pages retain browser local-file
origin restrictions; modules, fetches or applications requiring a development
server still need that server. Prewarming, retention timers, a script bridge,
remote resource proxies and built-in development servers are outside scope.

Markdown preview remains native and retains existing editing, navigation,
selection and layout behavior. Do not change global Windows rendering settings
without demonstrating why the change is needed and measuring its impact.

## Acceptance evidence

Capture the same local fixture before and after implementation, plus a Chrome
reference where possible. Exercise CSS layout, relative CSS/image/script loads,
JavaScript interaction, refresh, source switching, resizing and cleanup.

Measure comparable builds on the same machine. Record idle baseline, HTML source,
active preview, repeated navigation and closed preview. Include the application's
browser subprocesses in process-group memory; report resident and private committed
memory distinctly. Record preview load timing and idle CPU. Publish actual
measurements, environment, methodology and unverified cases in the PR.

//! One lazily-created browser view per window. The source editor remains the
//! owner of the file; the browser loads its saved, original file URL.

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

use gpui::{App, AppContext, Context, Entity, FocusHandle, Focusable, Render, Window, prelude::*};
use raw_window_handle::{RawWindowHandle, WindowHandle};

mod navigation;
#[cfg(windows)]
mod tooltips;
pub(crate) use navigation::{Event, Failure};

pub(crate) struct PendingHtmlPreview {
    path: PathBuf,
    webview: wry::WebView,
    started: Rc<Cell<Instant>>,
    observer: navigation::Observer,
}

pub(crate) struct HtmlPreview {
    pub(crate) path: PathBuf,
    pub(crate) owner: gpui::EntityId,
    loaded: bool,
    restore_pending: bool,
    laid_out: bool,
    pub(crate) failed: bool,
    view: Entity<gpui_wry::WebView>,
    started: Rc<Cell<Instant>>,
    observer: navigation::Observer,
    #[cfg(windows)]
    tooltips: tooltips::Tooltips,
}

impl HtmlPreview {
    /// Run on the foreground executor, outside an App update: WebView2 pumps
    /// Windows messages during construction, which may re-enter GPUI.
    pub(crate) fn build(
        path: &Path,
        parent: RawWindowHandle,
    ) -> anyhow::Result<PendingHtmlPreview> {
        let url = url::Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("HTML preview requires an absolute local file path"))?;
        let mut context = wry::WebContext::new(
            crate::core::config::config_dir_path().map(|dir| dir.join("html-preview")),
        );
        let started = Rc::new(Cell::new(Instant::now()));
        let builder = wry::WebViewBuilder::new_with_web_context(&mut context)
            .with_visible(false)
            .with_focused(false)
            .with_devtools(cfg!(debug_assertions))
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_on_page_load_handler({
                let started = started.clone();
                move |event, _| {
                    if matches!(event, wry::PageLoadEvent::Finished) {
                        let ms = started.get().elapsed().as_secs_f64() * 1000.;
                        log::debug!("HTML preview load finished in {ms:.2} ms");
                        #[cfg(feature = "markdown-visual-tests")]
                        eprintln!("HTML_PREVIEW_LOAD_MS={ms:.2}");
                    }
                }
            });
        // The handle comes from the current GPUI window and is used only
        // synchronously on its UI thread. No native handle crosses threads.
        let webview = builder.build_as_child(&unsafe { WindowHandle::borrow_raw(parent) })?;
        let observer = navigation::Observer::install(&webview)?;
        #[cfg(windows)]
        webview.load_url(url.as_str())?;
        #[cfg(target_os = "macos")]
        load_file_url(&webview, &url)?;
        Ok(PendingHtmlPreview {
            path: path.to_path_buf(),
            webview,
            started,
            observer,
        })
    }

    pub(crate) fn attach(
        pending: PendingHtmlPreview,
        owner: gpui::EntityId,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let PendingHtmlPreview {
            path,
            webview,
            started,
            observer,
        } = pending;
        let view = cx.new(|cx| gpui_wry::WebView::new(webview, window, cx));
        // The adapter starts visible internally even when the native builder
        // starts hidden. Keep both sides in agreement until it is laid out.
        view.update(cx, |view, _| view.hide());
        #[cfg(windows)]
        let tooltips = tooltips::Tooltips::new(&view, window, cx);
        Self {
            path,
            owner,
            loaded: false,
            restore_pending: true,
            laid_out: false,
            failed: false,
            view,
            started,
            observer,
            #[cfg(windows)]
            tooltips,
        }
    }

    pub(crate) fn navigate(
        &mut self,
        owner: gpui::EntityId,
        path: &Path,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        if self.path == path && self.owner == owner {
            return Ok(());
        }
        let url = url::Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("HTML preview requires an absolute local file path"))?;
        self.loaded = false;
        self.restore_pending = true;
        self.started.set(Instant::now());
        self.observer.begin();
        #[cfg(windows)]
        self.view.read(cx).raw().load_url(url.as_str())?;
        #[cfg(target_os = "macos")]
        load_file_url(self.view.read(cx).raw(), &url)?;
        self.path = path.to_path_buf();
        self.owner = owner;
        Ok(())
    }

    pub(crate) fn reload(&mut self, cx: &App) -> anyhow::Result<()> {
        self.loaded = false;
        self.restore_pending = true;
        self.started.set(Instant::now());
        self.observer.begin();
        self.view.read(cx).raw().reload()?;
        Ok(())
    }

    pub(crate) fn events(&self) -> smol::channel::Receiver<Event> {
        self.observer.events.clone()
    }

    pub(crate) fn mark_loaded(&mut self, request: u64) {
        if self.observer.is_current_ready(request) {
            self.loaded = true;
        }
    }

    pub(crate) fn needs_restore(&self) -> bool {
        self.loaded && self.laid_out && self.restore_pending
    }

    pub(crate) fn restored(&mut self) {
        self.restore_pending = false;
    }

    pub(crate) fn ready(&self) -> bool {
        self.loaded && !self.restore_pending && !self.failed
    }

    pub(crate) fn file_url(&self) -> String {
        url::Url::from_file_path(&self.path).unwrap().to_string()
    }

    pub(crate) fn capture_scroll(&self, cx: &App) -> Option<smol::channel::Receiver<String>> {
        // Same-document anchors change the URL without changing its owner.
        self.ready()
            .then(|| {
                self.evaluate(
                    "({url:location.href.split('#')[0],x:scrollX,y:scrollY})",
                    cx,
                )
                .ok()
            })
            .flatten()
    }

    pub(crate) fn restore_scroll(
        &self,
        position: [f64; 2],
        cx: &App,
    ) -> anyhow::Result<smol::channel::Receiver<String>> {
        let url = serde_json::to_string(&self.file_url())?;
        self.evaluate(&format!("(() => {{ if(location.href.split('#')[0] !== {url}) return false; window.scrollTo({{left:{},top:{},behavior:'instant'}}); return true; }})()", position[0], position[1]), cx)
    }

    fn evaluate(&self, script: &str, cx: &App) -> anyhow::Result<smol::channel::Receiver<String>> {
        let (tx, rx) = smol::channel::bounded(1);
        self.view
            .read(cx)
            .raw()
            .evaluate_script_with_callback(script, move |json| {
                let _ = tx.try_send(json);
            })?;
        Ok(rx)
    }

    pub(crate) fn is_current_failure(&self, failure: &Failure) -> bool {
        self.observer.is_current(failure)
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(crate) fn inspect(
        &self,
        script: &str,
        cx: &App,
    ) -> anyhow::Result<smol::channel::Receiver<String>> {
        self.evaluate(script, cx)
    }

    #[cfg(all(windows, feature = "markdown-visual-tests"))]
    pub(crate) fn tooltip_footprint(&self) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.tooltips.footprint()
    }

    #[cfg(all(windows, feature = "markdown-visual-tests"))]
    pub(crate) fn disable_tooltip_clipping(&self, cx: &App) {
        self.tooltips.disable(cx);
    }

    #[cfg(all(windows, feature = "markdown-visual-tests"))]
    pub(crate) fn native_covers(
        &self,
        point: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &App,
    ) -> bool {
        use windows_sys::Win32::Graphics::Gdi::{
            CreateRectRgn, DeleteObject, GetWindowRgn, PtInRegion,
        };
        use wry::WebViewExtWindows as _;
        let view = self.view.read(cx);
        assert!(
            view.visible(),
            "native preview must be visible for the occlusion probe"
        );
        assert!(
            view.bounds().contains(&point),
            "occlusion probe must lie inside the native preview: {point:?} {:?}",
            view.bounds()
        );
        let point = point - view.bounds().origin;
        let x = (point.x.as_f32() * window.scale_factor()).round() as i32;
        let y = (point.y.as_f32() * window.scale_factor()).round() as i32;
        let mut hwnd = Default::default();
        unsafe {
            view.raw().controller().ParentWindow(&mut hwnd).unwrap();
            let region = CreateRectRgn(0, 0, 0, 0);
            assert!(!region.is_null());
            let kind = GetWindowRgn(hwnd.0, region);
            let covers = kind == 0 || PtInRegion(region, x, y) != 0;
            DeleteObject(region);
            covers
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle(cx).focus(window, cx);
        if self.view.read(cx).visible() {
            let _ = self.view.read(cx).raw().focus();
        }
    }

    pub(crate) fn set_visible(&self, visible: bool, window: &Window, cx: &mut App) {
        let focused = self.focus_handle(cx).is_focused(window);
        self.view.update(cx, |view, _| {
            if view.visible() != visible {
                if visible {
                    view.show();
                    if focused {
                        let _ = view.raw().focus();
                    }
                } else {
                    view.hide();
                }
            }
        });
    }
}

#[cfg(target_os = "macos")]
fn load_file_url(webview: &wry::WebView, url: &url::Url) -> anyhow::Result<()> {
    use objc2_foundation::{NSString, NSURL};
    use wry::WebViewExtMacOS as _;

    let file = NSURL::URLWithString(&NSString::from_str(url.as_str()))
        .ok_or_else(|| anyhow::anyhow!("Invalid HTML file URL"))?;
    // A saved page can reference ../ resources anywhere on its local volume.
    // WebKit still applies its normal local-file origin restrictions. This
    // grants read access only; there is no application script bridge.
    let root = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/"), true);
    // WRY creates this WKWebView on the main thread. Both URLs are file URLs
    // and remain alive for the synchronous call, which retains them as needed.
    unsafe {
        webview
            .webview()
            .loadFileURL_allowingReadAccessToURL(&file, &root)
    }
    .ok_or_else(|| anyhow::anyhow!("WKWebView could not start loading the HTML file"))?;
    Ok(())
}

impl Focusable for HtmlPreview {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.view.read(cx).focus_handle(cx)
    }
}

impl Render for HtmlPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let this = cx.entity();
        let view = self.view.clone();
        gpui::div()
            .relative()
            .size_full()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus(window, cx)),
            )
            // gpui-wry skips layout while hidden. Size the native viewport
            // before restoring scroll, without briefly showing it at the top.
            .child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        if !view.read(cx).visible() {
                            let _ = view.read(cx).raw().set_bounds(wry::Rect {
                                size: wry::dpi::LogicalSize::new(
                                    f64::from(bounds.size.width),
                                    f64::from(bounds.size.height),
                                )
                                .into(),
                                position: wry::dpi::LogicalPosition::new(
                                    f64::from(bounds.origin.x),
                                    f64::from(bounds.origin.y),
                                )
                                .into(),
                            });
                        }
                        this.update(cx, |this, cx| {
                            #[cfg(windows)]
                            this.tooltips.layout(bounds, window, cx);
                            if !this.laid_out {
                                this.laid_out = true;
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(self.view.clone())
    }
}

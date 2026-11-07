use std::{cell::Cell, rc::Rc};

use objc2::{
    DeclaredClass, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_foundation::{MainThreadMarker, NSError, NSObject, NSObjectProtocol};
use objc2_web_kit::{
    WKDownload, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKNavigationResponse, WKNavigationResponsePolicy, WKWebView,
};

use super::{Event, Failure};

pub(crate) struct State {
    original: Retained<ProtocolObject<dyn WKNavigationDelegate>>,
    request: Rc<Cell<u64>>,
    latest: Cell<(usize, u64)>,
    tx: smol::channel::Sender<Event>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "Ctty7HtmlNavigationDelegate"]
    #[thread_kind = MainThreadOnly]
    #[ivars = State]
    pub(crate) struct NavigationDelegate;

    unsafe impl NSObjectProtocol for NavigationDelegate {}

    unsafe impl WKNavigationDelegate for NavigationDelegate {
        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn started(&self, _: &WKWebView, navigation: Option<&WKNavigation>) {
            self.ivars()
                .latest
                .set((navigation_id(navigation), self.ivars().request.get()));
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_provisional(
            &self,
            _: &WKWebView,
            navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.record_failure(navigation, error);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed_navigation(
            &self,
            _: &WKWebView,
            navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.record_failure(navigation, error);
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn terminated(&self, _: &WKWebView) {
            let _ = self.ivars().tx.try_send(Event::Failed(Failure {
                request: self.ivars().request.get(),
                message: "WKWebView content process terminated".into(),
            }));
        }

        // Preserve WRY's policy, script-queue, timing and download handling.
        // Its delegate does not implement the additional failure methods above.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn action(
            &self,
            view: &WKWebView,
            action: &WKNavigationAction,
            handler: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            unsafe {
                self.ivars()
                    .original
                    .webView_decidePolicyForNavigationAction_decisionHandler(view, action, handler);
            }
        }

        #[unsafe(method(webView:decidePolicyForNavigationResponse:decisionHandler:))]
        fn response(
            &self,
            view: &WKWebView,
            response: &WKNavigationResponse,
            handler: &block2::DynBlock<dyn Fn(WKNavigationResponsePolicy)>,
        ) {
            unsafe {
                self.ivars()
                    .original
                    .webView_decidePolicyForNavigationResponse_decisionHandler(
                        view, response, handler,
                    );
            }
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn committed(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
            unsafe {
                self.ivars()
                    .original
                    .webView_didCommitNavigation(view, navigation);
            }
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
            unsafe {
                self.ivars()
                    .original
                    .webView_didFinishNavigation(view, navigation);
            }
            let (id, request) = self.ivars().latest.get();
            if navigation_id(navigation) == id {
                let _ = self.ivars().tx.try_send(Event::Ready(request));
            }
        }

        #[unsafe(method(webView:navigationAction:didBecomeDownload:))]
        fn action_download(
            &self,
            view: &WKWebView,
            action: &WKNavigationAction,
            download: &WKDownload,
        ) {
            unsafe {
                self.ivars()
                    .original
                    .webView_navigationAction_didBecomeDownload(view, action, download);
            }
        }

        #[unsafe(method(webView:navigationResponse:didBecomeDownload:))]
        fn response_download(
            &self,
            view: &WKWebView,
            response: &WKNavigationResponse,
            download: &WKDownload,
        ) {
            unsafe {
                self.ivars()
                    .original
                    .webView_navigationResponse_didBecomeDownload(view, response, download);
            }
        }
    }
);

impl NavigationDelegate {
    fn record_failure(&self, navigation: Option<&WKNavigation>, error: &NSError) {
        let (id, request) = self.ivars().latest.get();
        // NSURLErrorCancelled is expected when a newer navigation supersedes it.
        if error.code() != -999 && navigation_id(navigation) == id {
            let _ = self.ivars().tx.try_send(Event::Failed(Failure {
                request,
                message: error.localizedDescription().to_string(),
            }));
        }
    }
}

fn navigation_id(navigation: Option<&WKNavigation>) -> usize {
    navigation.map_or(0, |navigation| navigation as *const WKNavigation as usize)
}

pub(super) fn install(
    webview: &wry::WebView,
    request: Rc<Cell<u64>>,
    tx: smol::channel::Sender<Event>,
) -> anyhow::Result<Retained<NavigationDelegate>> {
    use wry::WebViewExtMacOS as _;
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("WKWebView requires the main thread"))?;
    let view = webview.webview();
    // WKWebView keeps its delegate weakly. Retain both our observer and WRY's
    // existing delegate, and only forward the methods that WRY implements.
    unsafe {
        let original = view
            .navigationDelegate()
            .ok_or_else(|| anyhow::anyhow!("WKWebView navigation delegate is unavailable"))?;
        let delegate = mtm.alloc::<NavigationDelegate>().set_ivars(State {
            original,
            request,
            latest: Cell::new((0, 0)),
            tx,
        });
        let delegate: Retained<NavigationDelegate> = msg_send![super(delegate), init];
        view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Ok(delegate)
    }
}

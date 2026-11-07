//! Native main-frame load events. Each event keeps the request that started
//! its navigation, so an older page cannot reset a newly selected document.

use std::{cell::Cell, rc::Rc};

#[cfg(target_os = "macos")]
#[path = "navigation/macos.rs"]
mod macos;

pub(crate) struct Failure {
    request: u64,
    pub(crate) message: String,
}

pub(crate) enum Event {
    Ready(u64),
    Failed(Failure),
}

pub(super) struct Observer {
    request: Rc<Cell<u64>>,
    pub(super) events: smol::channel::Receiver<Event>,
    #[cfg(target_os = "macos")]
    _delegate: objc2::rc::Retained<macos::NavigationDelegate>,
}

impl Observer {
    pub(super) fn install(webview: &wry::WebView) -> anyhow::Result<Self> {
        let request = Rc::new(Cell::new(1));
        let (tx, events) = smol::channel::unbounded();
        #[cfg(windows)]
        windows(webview, request.clone(), tx)?;
        Ok(Self {
            #[cfg(target_os = "macos")]
            _delegate: macos::install(webview, request.clone(), tx)?,
            request,
            events,
        })
    }

    pub(super) fn begin(&self) {
        self.request.set(self.request.get().wrapping_add(1));
    }

    pub(super) fn is_current(&self, failure: &Failure) -> bool {
        self.request.get() == failure.request
    }

    pub(super) fn is_current_ready(&self, request: u64) -> bool {
        self.request.get() == request
    }
}

#[cfg(windows)]
fn windows(
    webview: &wry::WebView,
    request: Rc<Cell<u64>>,
    tx: smol::channel::Sender<Event>,
) -> anyhow::Result<()> {
    use webview2_com::{
        NavigationCompletedEventHandler, NavigationStartingEventHandler, ProcessFailedEventHandler,
    };
    use wry::WebViewExtWindows as _;

    let view = webview.webview();
    let latest = Rc::new(Cell::new((0, 0)));
    let mut token = 0;
    // WebView2 invokes these handlers on its owning UI thread. Keep the
    // navigation id paired with the application request captured at start.
    unsafe {
        view.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new({
                let latest = latest.clone();
                let request = request.clone();
                move |_, args| {
                    if let Some(args) = args {
                        let mut id = 0;
                        args.NavigationId(&mut id)?;
                        latest.set((id, request.get()));
                    }
                    Ok(())
                }
            })),
            &mut token,
        )?;
        view.add_NavigationCompleted(&NavigationCompletedEventHandler::create(Box::new({
            let tx = tx.clone();
            move |_, args| {
                if let Some(args) = args {
                    let mut id = 0;
                    let mut success = Default::default();
                    let mut status = Default::default();
                    args.NavigationId(&mut id)?;
                    args.IsSuccess(&mut success)?;
                    args.WebErrorStatus(&mut status)?;
                    if success.as_bool() && id == latest.get().0 {
                        let _ = tx.try_send(Event::Ready(latest.get().1));
                    }
                    // Aborted navigation is normal when switching files.
                    if !success.as_bool() && status != webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED && id == latest.get().0 {
                        let _ = tx.try_send(Event::Failed(Failure { request: latest.get().1,
                            message: format!("WebView2 navigation failed ({status:?})") }));
                    }
                }
                Ok(())
            }
        })), &mut token)?;
        view.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, _| {
                let _ = tx.try_send(Event::Failed(Failure {
                    request: request.get(),
                    message: "WebView2 browser process failed".into(),
                }));
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}

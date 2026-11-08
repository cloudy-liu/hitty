//! WebView2's child HWND sits above GPUI. Exclude the shared tooltip's
//! footprint from that child, keeping the rest of the live page interactive.

use std::{cell::RefCell, rc::Rc};

use gpui::{App, Bounds, Entity, Pixels, Subscription, WeakEntity, Window};
use gpui_component::Root;
use windows_sys::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, RGN_DIFF, SetWindowRgn,
};
use wry::WebViewExtWindows as _;

pub(super) struct Tooltips {
    state: Rc<RefCell<State>>,
    _subscription: Subscription,
}

struct State {
    view: WeakEntity<gpui_wry::WebView>,
    page: Bounds<Pixels>,
    tooltip: Option<Bounds<Pixels>>,
    scale: f32,
    applied: Option<([i32; 2], [i32; 4])>,
    #[cfg(feature = "markdown-visual-tests")]
    disabled: bool,
}

impl Tooltips {
    pub(super) fn new(view: &Entity<gpui_wry::WebView>, window: &Window, cx: &mut App) -> Self {
        let state = Rc::new(RefCell::new(State {
            view: view.downgrade(),
            page: Bounds::default(),
            tooltip: None,
            scale: window.scale_factor(),
            applied: None,
            #[cfg(feature = "markdown-visual-tests")]
            disabled: false,
        }));
        let subscription = Root::observe_tooltip_bounds(window, cx, {
            let state = state.clone();
            move |tooltip, cx| {
                let mut state = state.borrow_mut();
                state.tooltip = tooltip;
                state.apply(cx);
            }
        })
        .expect("HTML preview requires a component Root");
        Self {
            state,
            _subscription: subscription,
        }
    }

    pub(super) fn layout(&self, page: Bounds<Pixels>, window: &Window, cx: &App) {
        let mut state = self.state.borrow_mut();
        state.page = page;
        state.scale = window.scale_factor();
        state.apply(cx);
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(super) fn footprint(&self) -> Option<Bounds<Pixels>> {
        let state = self.state.borrow();
        state
            .tooltip
            .map(|tooltip| state.page.intersect(&tooltip))
            .filter(|bounds| !bounds.is_empty())
    }

    #[cfg(feature = "markdown-visual-tests")]
    pub(super) fn disable(&self, cx: &App) {
        let mut state = self.state.borrow_mut();
        state.disabled = true;
        state.apply(cx);
    }
}

impl State {
    fn apply(&mut self, cx: &App) {
        let size = [
            (self.page.size.width.as_f32() * self.scale).round() as i32,
            (self.page.size.height.as_f32() * self.scale).round() as i32,
        ];
        let desired = self
            .tooltip
            .and_then(|tooltip| native_clip(self.page, tooltip, self.scale))
            .map(|clip| (size, clip));
        #[cfg(feature = "markdown-visual-tests")]
        let desired = if self.disabled { None } else { desired };
        if desired == self.applied {
            return;
        }
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let controller = view.read(cx).raw().controller();
        let mut hwnd = Default::default();
        // In WRY child mode this is the WebView container, not the application
        // HWND. Regions use physical, child-local coordinates.
        unsafe {
            if controller.ParentWindow(&mut hwnd).is_err() {
                return;
            }
            let region = if let Some(([width, height], [left, top, right, bottom])) = desired {
                let region = CreateRectRgn(0, 0, width, height);
                if region.is_null() {
                    return;
                }
                let cut = CreateRectRgn(left, top, right, bottom);
                if cut.is_null() || CombineRgn(region, region, cut, RGN_DIFF) == 0 {
                    if !cut.is_null() {
                        DeleteObject(cut);
                    }
                    DeleteObject(region);
                    return;
                }
                DeleteObject(cut);
                region
            } else {
                std::ptr::null_mut()
            };
            if SetWindowRgn(hwnd.0, region, 1) == 0 {
                if !region.is_null() {
                    DeleteObject(region);
                }
                log::warn!("Could not update the HTML preview tooltip region");
                return;
            }
            // A successful SetWindowRgn transfers region ownership to Windows.
        }
        self.applied = desired;
    }
}

fn native_clip(page: Bounds<Pixels>, tooltip: Bounds<Pixels>, scale: f32) -> Option<[i32; 4]> {
    let overlap = page.intersect(&tooltip);
    if overlap.is_empty() {
        return None;
    }
    Some([
        ((overlap.left() - page.left()).as_f32() * scale).floor() as i32,
        ((overlap.top() - page.top()).as_f32() * scale).floor() as i32,
        ((overlap.right() - page.left()).as_f32() * scale).ceil() as i32,
        ((overlap.bottom() - page.top()).as_f32() * scale).ceil() as i32,
    ])
}

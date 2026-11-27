//! Multi-colour editor and file-manager icons rasterised at exact device pixels.
//!
//! Same approach as [`crate::ui::file_icons`]: GPUI renders `Icon`/`svg()` as a
//! single-colour alpha mask, so multi-colour brand marks go through `resvg`
//! into cached `RenderImage` instances at the window's scale factor.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{
    AnyElement, ImageSource, IntoElement as _, Pixels, RenderImage, Styled as _, Window, div, img,
};

use crate::core::open_in::OpenInApp;
use crate::ui::file_icons::rasterize;

static RASTERS: LazyLock<Mutex<HashMap<(OpenInApp, bool, u32), Option<Arc<RenderImage>>>>> =
    LazyLock::new(Default::default);

fn svg_bytes(app: OpenInApp, dark: bool) -> &'static [u8] {
    match app {
        OpenInApp::VsCode => include_bytes!("../../assets/icons/editors/vscode.svg"),
        OpenInApp::VsCodeInsiders => {
            include_bytes!("../../assets/icons/editors/vscode-insiders.svg")
        }
        OpenInApp::Cursor => {
            if dark {
                include_bytes!("../../assets/icons/editors/cursor-dark.svg")
            } else {
                include_bytes!("../../assets/icons/editors/cursor-light.svg")
            }
        }
        OpenInApp::Windsurf => include_bytes!("../../assets/icons/editors/windsurf.svg"),
        OpenInApp::Zed => include_bytes!("../../assets/icons/editors/zed.svg"),
        OpenInApp::IntelliJ => include_bytes!("../../assets/icons/editors/intellij.svg"),
        OpenInApp::CLion => include_bytes!("../../assets/icons/editors/clion.svg"),
        OpenInApp::RustRover => include_bytes!("../../assets/icons/editors/rustrover.svg"),
        OpenInApp::PyCharm => include_bytes!("../../assets/icons/editors/pycharm.svg"),
        OpenInApp::WebStorm => include_bytes!("../../assets/icons/editors/webstorm.svg"),
        OpenInApp::GoLand => include_bytes!("../../assets/icons/editors/goland.svg"),
        OpenInApp::Qoder => include_bytes!("../../assets/icons/agents/qoder-startup.svg"),
        OpenInApp::SublimeText => include_bytes!("../../assets/icons/editors/sublime.svg"),
        OpenInApp::FileManager => include_bytes!("../../assets/icons/editors/file-manager.svg"),
    }
}

/// Render `app`'s coloured icon at `size` logical pixels for the current scale
/// factor and surface polarity (`dark`).
pub(crate) fn render(app: OpenInApp, size: Pixels, dark: bool, window: &Window) -> AnyElement {
    let device = (f32::from(size) * window.scale_factor()).round().max(1.) as u32;
    match raster(app, dark, device) {
        Some(image) => img(ImageSource::Render(image))
            .size(size)
            .flex_none()
            .into_any_element(),
        None => div().size(size).flex_none().into_any_element(),
    }
}

fn raster(app: OpenInApp, dark: bool, device: u32) -> Option<Arc<RenderImage>> {
    // Only Cursor adapts its faces to dark vs light surfaces; every other mark
    // shares one raster across both polarities.
    let key_dark = matches!(app, OpenInApp::Cursor) && dark;
    RASTERS
        .lock()
        .unwrap()
        .entry((app, key_dark, device))
        .or_insert_with(|| rasterize(svg_bytes(app, key_dark), device).map(Arc::new))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_editor_icon_rasterizes_non_empty_in_both_polarities() {
        for &app in OpenInApp::ALL_EDITORS
            .iter()
            .chain(std::iter::once(&OpenInApp::FileManager))
        {
            for dark in [false, true] {
                let img = rasterize(svg_bytes(app, dark), 28)
                    .unwrap_or_else(|| panic!("failed to rasterize {app:?} (dark={dark})"));
                let size = img.size(0);
                assert_eq!((size.width.0, size.height.0), (28, 28));
            }
        }
    }
}

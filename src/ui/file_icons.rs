//! File and folder icons from the vendored Symbols theme
//! (`assets/file-icons/symbols`, MIT, see its LICENSE).
//!
//! The theme is a VS Code icon theme and is read the way VS Code reads one:
//! a file is matched by its whole name, then by each extension from longest to
//! shortest (`a.d.ts` tries `d.ts`, then `ts`), then gets the default; a folder
//! by its name, then the default. Names are compared lowercased.
//!
//! The icons are multi-colour, which `Icon`/`svg()` cannot draw: gpui renders
//! those as an alpha mask tinted with one colour. They go through `img()`
//! instead, rasterised here at the exact device size. Letting `img()` load the
//! SVG itself would rasterise at twice its intrinsic 24px and leave the GPU to
//! shrink that 3x into a 16px row, which blurs the hairlines.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{
    AnyElement, ImageSource, IntoElement as _, Pixels, RenderImage, Styled as _, Window, div, img,
};
use image::{Frame, RgbaImage};
use resvg::{tiny_skia, usvg};
use serde::Deserialize;
use smallvec::SmallVec;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));
}

/// The icon's size on a list row. Symbols draws on a 24-unit grid with about
/// 18 units of ink, sized at 16px beside the panel's file names.
pub(crate) const ROW_ICON: f32 = 16.0;

const THEME_JSON: &str = include_str!("../../assets/file-icons/symbols/symbol-icon-theme.json");

/// One icon of the theme, named by its key in the embedded table
/// (`files/rust.svg`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FileIcon(&'static str);

impl FileIcon {
    /// The icon for a file called `name`.
    pub(crate) fn for_file(name: &str) -> Self {
        let theme = &*THEME;
        let name = name.to_lowercase();
        if let Some(icon) = theme.file_names.get(&name) {
            return *icon;
        }
        let mut rest = name.as_str();
        while let Some((_, ext)) = rest.split_once('.') {
            if let Some(icon) = theme.file_extensions.get(ext) {
                return *icon;
            }
            rest = ext;
        }
        theme.file
    }

    /// The icon for a folder called `name`. The root of a tree takes the
    /// theme's root icon whatever it is called.
    pub(crate) fn for_dir(name: &str, is_root: bool) -> Self {
        let theme = &*THEME;
        if is_root {
            return theme.root_folder;
        }
        theme
            .folder_names
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or(theme.folder)
    }

    fn svg(self) -> &'static [u8] {
        lookup(self.0)
            .expect("FileIcon is only built from embedded keys")
            .1
    }

    /// The icon as an element `size` across.
    pub(crate) fn render(self, size: Pixels, window: &Window) -> AnyElement {
        let device = (f32::from(size) * window.scale_factor()).round().max(1.) as u32;
        match raster(self, device) {
            Some(image) => img(ImageSource::Render(image))
                .size(size)
                .flex_none()
                .into_any_element(),
            // An SVG resvg cannot parse keeps its cell, so the name still lines up.
            None => div().size(size).flex_none().into_any_element(),
        }
    }
}

fn lookup(key: &str) -> Option<&'static (&'static str, &'static [u8])> {
    embedded::SVGS
        .binary_search_by(|(k, _)| k.cmp(&key))
        .ok()
        .map(|ix| &embedded::SVGS[ix])
}

struct Theme {
    file: FileIcon,
    folder: FileIcon,
    root_folder: FileIcon,
    file_names: HashMap<String, FileIcon>,
    file_extensions: HashMap<String, FileIcon>,
    folder_names: HashMap<String, FileIcon>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeJson {
    icon_definitions: HashMap<String, IconDefinition>,
    file: String,
    folder: String,
    root_folder: String,
    file_extensions: HashMap<String, String>,
    file_names: HashMap<String, String>,
    folder_names: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IconDefinition {
    icon_path: String,
}

static THEME: LazyLock<Theme> = LazyLock::new(|| {
    let json: ThemeJson = serde_json::from_str(THEME_JSON).expect("vendored theme parses");
    // Definition id → embedded icon. Upstream maps a couple of names to ids it
    // never defines; those drop out here, so the name falls through to the
    // next rule instead of drawing nothing.
    let icon = |id: &str| {
        let path = json.icon_definitions.get(id)?.icon_path.as_str();
        let key = path.trim_start_matches("./").trim_start_matches("icons/");
        lookup(key).map(|(k, _)| FileIcon(k))
    };
    let table = |map: &HashMap<String, String>| {
        map.iter()
            .filter_map(|(name, id)| Some((name.to_lowercase(), icon(id)?)))
            .collect()
    };
    Theme {
        file: icon(&json.file).expect("theme has a default file icon"),
        folder: icon(&json.folder).expect("theme has a default folder icon"),
        root_folder: icon(&json.root_folder).expect("theme has a root folder icon"),
        file_names: table(&json.file_names),
        file_extensions: table(&json.file_extensions),
        folder_names: table(&json.folder_names),
    }
});

/// Rasters by icon and device size. A tree only ever shows a few dozen
/// distinct icons at one or two scale factors, so nothing is evicted, and the
/// atlas keeps each image under a stable id rather than re-uploading per frame.
static RASTERS: LazyLock<Mutex<HashMap<(FileIcon, u32), Option<Arc<RenderImage>>>>> =
    LazyLock::new(Default::default);

fn raster(icon: FileIcon, device: u32) -> Option<Arc<RenderImage>> {
    RASTERS
        .lock()
        .unwrap()
        .entry((icon, device))
        .or_insert_with(|| rasterize(icon.svg(), device).map(Arc::new))
        .clone()
}

fn rasterize(svg: &[u8], size: u32) -> Option<RenderImage> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = size as f32 / w.max(h);
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(
        (size as f32 - w * scale) / 2.,
        (size as f32 - h * scale) / 2.,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // `RenderImage` holds straight-alpha BGRA; tiny-skia hands back
    // premultiplied RGBA.
    let mut data = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        data.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
    }
    let buffer = RgbaImage::from_raw(size, size, data)?;
    Some(RenderImage::new(SmallVec::from_elem(Frame::new(buffer), 1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_reference_resolves_to_an_embedded_svg() {
        let json: ThemeJson = serde_json::from_str(THEME_JSON).unwrap();
        let mut missing: Vec<&str> = json
            .icon_definitions
            .iter()
            .filter(|(_, def)| {
                let key = def
                    .icon_path
                    .trim_start_matches("./")
                    .trim_start_matches("icons/");
                lookup(key).is_none()
            })
            .map(|(id, _)| id.as_str())
            .collect();
        missing.sort();
        assert!(missing.is_empty(), "defined but not embedded: {missing:?}");
    }

    #[test]
    fn this_repo_gets_its_own_icons() {
        for (name, icon) in [
            ("main.rs", "files/rust.svg"),
            ("Cargo.toml", "files/gear.svg"),
            ("Cargo.lock", "files/lock.svg"),
            ("README.md", "files/markdown.svg"),
            (".gitignore", "files/git.svg"),
            ("LICENSE", "files/license.svg"),
            ("index.d.ts", "files/ts-types.svg"),
            ("no-extension", "files/document.svg"),
            ("weird.unknownext", "files/document.svg"),
        ] {
            assert_eq!(FileIcon::for_file(name).0, icon, "{name}");
        }
        for (name, icon) in [
            ("src", "folders/folder-orange-code.svg"),
            ("assets", "folders/folder-assets.svg"),
            ("docs", "folders/folder-documents.svg"),
            (".github", "folders/folder-github.svg"),
            ("crates", "folders/folder.svg"),
        ] {
            assert_eq!(FileIcon::for_dir(name, false).0, icon, "{name}");
        }
        assert_eq!(FileIcon::for_dir("src", true).0, "folders/folder-gray.svg");
    }

    #[test]
    fn icons_keep_their_colour_at_the_device_size() {
        let image = rasterize(FileIcon::for_file("main.rs").svg(), 24).unwrap();
        assert_eq!(image.size(0), gpui::size(24.into(), 24.into()));
        // rust.svg is filled #EA580C; stored as BGRA.
        let pixels = image.as_bytes(0).unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .any(|p| p == [0x0c, 0x58, 0xea, 0xff]),
            "no opaque #EA580C pixel in the Rust icon"
        );
    }
}

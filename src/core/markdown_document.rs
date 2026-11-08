//! URL interpretation for a document with an explicit owning Host. Relative
//! resources never become a local `file:` URL, even for remote documents.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Web(String),
    Anchor(String),
    File {
        path: PathBuf,
        fragment: Option<String>,
    },
}

fn decode(value: &str) -> Result<String, String> {
    let value = percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .map_err(|_| "link is not valid UTF-8")?
        .into_owned();
    if value.chars().any(char::is_control) {
        return Err("link contains a control character".into());
    }
    Ok(value)
}

pub fn resolve(document: &Path, target: &str, local: bool) -> Result<Target, String> {
    let target = target.trim();
    if let Some(anchor) = target.strip_prefix('#') {
        return Ok(Target::Anchor(decode(anchor)?));
    }
    if target.starts_with("//") {
        return resolve(document, &format!("https:{target}"), local);
    }
    let windows_path = local
        && target.as_bytes().get(1) == Some(&b':')
        && target
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
        && target
            .as_bytes()
            .get(2)
            .is_some_and(|c| *c == b'/' || *c == b'\\');
    if !windows_path && let Ok(url) = url::Url::parse(target) {
        return if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
            Ok(Target::Web(url.to_string()))
        } else {
            Err(format!("unsupported link scheme: {}", url.scheme()))
        };
    }
    let (path, fragment) = target
        .split_once('#')
        .map_or((target, None), |(p, f)| (p, Some(f)));
    let path = path.split_once('?').map_or(path, |(p, _)| p);
    let path = decode(path)?;
    let fragment = fragment.map(decode).transpose()?;
    if path.is_empty() {
        return Ok(Target::Anchor(fragment.unwrap_or_default()));
    }
    // A colon in the first relative segment denotes an unsupported or malformed
    // scheme. Do not accidentally pass it to an OS URL/file opener.
    if !windows_path
        && path
            .split(['/', '\\'])
            .next()
            .is_some_and(|p| p.contains(':'))
    {
        return Err("invalid document link".into());
    }
    let path = if local {
        document.parent().unwrap_or(Path::new(".")).join(path)
    } else {
        // Host paths use POSIX separators regardless of the client's platform.
        if path.contains('\\') {
            return Err("remote document paths must use '/'".into());
        }
        if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            let doc = document.to_string_lossy().replace('\\', "/");
            let parent = doc.rsplit_once('/').map_or(".", |(parent, _)| parent);
            PathBuf::from(format!("{parent}/{path}"))
        }
    };
    Ok(Target::File { path, fragment })
}

pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

/// Validate the format and dimensions before sending encoded data to GPUI.
pub fn image_format(bytes: &[u8]) -> Result<gpui::ImageFormat, String> {
    if std::str::from_utf8(bytes).ok().and_then(|text| {
        resvg::usvg::roxmltree::Document::parse(text)
            .ok()
            .map(|doc| {
                let root = doc.root_element();
                root.tag_name().name() == "svg"
                    && matches!(
                        root.tag_name().namespace(),
                        None | Some("http://www.w3.org/2000/svg")
                    )
            })
    }) == Some(true)
    {
        // Use the renderer's parser for units, percentages and viewBox sizing.
        // Dimension validation must not load nested images or local files.
        let options = resvg::usvg::Options {
            image_href_resolver: resvg::usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..Default::default()
        };
        let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
        // GPUI's render_single_frame(bytes, 1.0) doubles each dimension for
        // smoothing. Bound the allocated pixels, not just the SVG viewport.
        let width = (tree.size().width() * 2.0) as u32;
        let height = (tree.size().height() * 2.0) as u32;
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 32_000_000 {
            return Err("SVG render exceeds image dimensions limit".into());
        }
        return Ok(gpui::ImageFormat::Svg);
    }
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if u64::from(width) * u64::from(height) > 32_000_000 {
        return Err("image exceeds 32 megapixels".into());
    }
    use gpui::ImageFormat as G;
    use image::ImageFormat as I;
    Ok(match format {
        I::Png => G::Png,
        I::Jpeg => G::Jpeg,
        I::Gif => G::Gif,
        I::WebP => G::Webp,
        I::Bmp => G::Bmp,
        I::Tiff => G::Tiff,
        I::Ico => G::Ico,
        I::Pnm => G::Pnm,
        _ => return Err("unsupported image format".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_svg_accepts_xml_prologs_and_rejects_other_xml() {
        for prolog in [
            "<!-- generator -->",
            "\u{feff}",
            "<?xml version=\"1.0\"?><!-- generator -->",
        ] {
            let svg = format!(
                "{prolog}<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"/>"
            );
            assert!(matches!(
                image_format(svg.as_bytes()),
                Ok(gpui::ImageFormat::Svg)
            ));
        }
        assert!(image_format(b"<!-- svg --><document/>").is_err());
        assert!(image_format(b"<!-- unclosed <svg/>").is_err());
    }

    #[test]
    fn markdown_svg_dimensions_bound_the_gpui_render_allocation() {
        for dimensions in [
            "width=\"10000\" height=\"10000\"",
            "viewBox=\"0 0 10000 10000\"",
            "width=\"4000\" height=\"4000\"",
            "width=\"100in\" height=\"100in\"",
            "width=\"1e30\" height=\"1e30\"",
            "width=\"0\" height=\"16\"",
        ] {
            let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" {dimensions}/>");
            assert!(image_format(svg.as_bytes()).is_err(), "{dimensions}");
        }
        for dimensions in [
            "width=\"16\" height=\"16\"",
            "viewBox=\"0 0 16 16\"",
            "width=\"4000\" height=\"2000\"",
        ] {
            let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" {dimensions}/>");
            assert!(
                matches!(image_format(svg.as_bytes()), Ok(gpui::ImageFormat::Svg)),
                "{dimensions}"
            );
        }
        assert!(image_format(b"<svg not valid XML").is_err());
    }

    #[test]
    fn markdown_links_keep_the_document_directory_and_decode_fragments() {
        assert_eq!(
            resolve(Path::new("/repo/docs/readme.md"), "images/a%20b.png", false).unwrap(),
            Target::File {
                path: "/repo/docs/images/a b.png".into(),
                fragment: None
            }
        );
        assert_eq!(
            resolve(
                Path::new("/repo/docs/readme.md"),
                "../other.md#%E6%A0%87%E9%A2%98",
                false
            )
            .unwrap(),
            Target::File {
                path: "/repo/docs/../other.md".into(),
                fragment: Some("标题".into())
            }
        );
        assert_eq!(
            resolve(Path::new("/repo/readme.md"), "#hello-world-1", true).unwrap(),
            Target::Anchor("hello-world-1".into())
        );
        assert_eq!(
            resolve(Path::new("/repo/readme.md"), "/elsewhere/intro.md", false).unwrap(),
            Target::File {
                path: "/elsewhere/intro.md".into(),
                fragment: None
            }
        );
        assert_eq!(
            resolve(Path::new("docs/readme.md"), "a%23b.md#intro", true).unwrap(),
            Target::File {
                path: Path::new("docs").join("a#b.md"),
                fragment: Some("intro".into())
            }
        );
    }

    #[test]
    fn markdown_links_only_use_the_browser_for_web_urls() {
        let doc = Path::new("/repo/readme.md");
        assert!(matches!(
            resolve(doc, "https://example.com/a#b", false),
            Ok(Target::Web(_))
        ));
        for link in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:image/svg+xml,test",
            "vscode://file/test",
            "bad%00path",
            "C:\\local\\image.png",
        ] {
            assert!(resolve(doc, link, false).is_err(), "{link}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn markdown_windows_relative_paths_are_resolved_on_the_owning_host() {
        assert_eq!(
            resolve(
                Path::new("D:\\project\\docs\\readme.md"),
                "images/picture.png",
                true
            )
            .unwrap(),
            Target::File {
                path: Path::new("D:\\project\\docs").join("images/picture.png"),
                fragment: None
            }
        );
    }
}

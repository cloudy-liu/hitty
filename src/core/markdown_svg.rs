//! Bounded raster playback for looping SMIL value animations. XML and path
//! syntax are parsed by the same libraries used by the static SVG renderer.

use resvg::usvg::roxmltree::{Document, Node};
use std::{
    ops::Range,
    sync::{Arc, LazyLock},
};

const MAX_FRAMES: usize = 240;
const MAX_PIXELS: u64 = 8_000_000;

pub fn render(bytes: &[u8]) -> Result<Option<Arc<gpui::RenderImage>>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let doc = Document::parse(text).map_err(|e| e.to_string())?;
    let nodes: Vec<_> = doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && matches!(
                    n.tag_name().name(),
                    "animate" | "animateTransform" | "animateMotion" | "set"
                )
        })
        .collect();
    if nodes.is_empty() {
        return Ok(None);
    }
    if nodes.len() > 32 {
        return Err("SVG animation limit reached (32)".into());
    }
    let tracks = nodes
        .iter()
        .map(|n| Track::parse(*n))
        .collect::<Result<Vec<_>, _>>()?;
    let duration = tracks[0].duration;
    if tracks
        .iter()
        .any(|track| (track.duration - duration).abs() > 0.001)
    {
        return Err("SVG animations must share a loop duration".into());
    }
    let options = options(&doc)?;
    let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let pixels = u64::from(size.width()) * u64::from(size.height());
    let count = (duration * 20.).ceil().max(2.) as usize;
    if count > MAX_FRAMES || pixels * count as u64 > MAX_PIXELS {
        return Err("SVG animation exceeds frame or memory limit".into());
    }
    let mut frames = smallvec::SmallVec::<[image::Frame; 1]>::new();
    for i in 0..count {
        let snapshot = snapshot(text, &nodes, &tracks, i as f64 / count as f64)?;
        let tree = resvg::usvg::Tree::from_str(&snapshot, &options).map_err(|e| e.to_string())?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or("cannot allocate SVG frame")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let mut data = pixmap.take();
        for pixel in data.chunks_exact_mut(4) {
            gpui::swap_rgba_pa_to_bgra(pixel);
        }
        let buffer = image::RgbaImage::from_raw(size.width(), size.height(), data)
            .ok_or("invalid SVG frame")?;
        frames.push(image::Frame::from_parts(
            buffer,
            0,
            0,
            image::Delay::from_numer_denom_ms((duration * 1000.).round() as u32, count as u32),
        ));
    }
    Ok(Some(Arc::new(gpui::RenderImage::new(frames))))
}

fn options(doc: &Document<'_>) -> Result<resvg::usvg::Options<'static>, String> {
    static FONTS: LazyLock<Arc<resvg::usvg::fontdb::Database>> = LazyLock::new(|| {
        let mut fonts = resvg::usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    });
    let embedded = embedded_fonts(doc)?;
    let mut fontdb = FONTS.clone();
    if embedded.faces().next().is_some() {
        let db = Arc::make_mut(&mut fontdb);
        // A document's web font must win over an installed face of the same
        // family. Keep this override private to the document, including aliases.
        let replaced: Vec<_> = db
            .faces()
            .filter(|face| {
                embedded.faces().any(|web| {
                    face.families
                        .iter()
                        .any(|(name, _)| web.families.iter().any(|(other, _)| name == other))
                })
            })
            .map(|face| face.id)
            .collect();
        for id in replaced {
            db.remove_face(id);
        }
        for face in embedded.faces() {
            db.push_face_info(face.clone());
        }
    }
    Ok(resvg::usvg::Options {
        fontdb,
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    })
}

fn embedded_fonts(doc: &Document<'_>) -> Result<resvg::usvg::fontdb::Database, String> {
    use base64::Engine;
    use cssparser::{Parser, ParserInput, Token};
    let mut fonts = resvg::usvg::fontdb::Database::new();
    let mut total = 0;
    for style in doc.descendants().filter(|n| n.has_tag_name("style")) {
        let mut input = ParserInput::new(style.text().unwrap_or_default());
        let mut parser = Parser::new(&mut input);
        while let Ok(token) = parser.next() {
            if !matches!(token, Token::AtKeyword(name) if name.eq_ignore_ascii_case("font-face")) {
                continue;
            }
            if parser.expect_curly_bracket_block().is_err() {
                continue;
            }
            let Ok((Some(family), sources)) = parser.parse_nested_block(font_face) else {
                continue;
            };
            for url in sources {
                let Some((header, encoded)) = url.split_once(',') else {
                    continue;
                };
                if ![
                    "data:font/ttf;base64",
                    "data:font/truetype;base64",
                    "data:font/otf;base64",
                    "data:font/opentype;base64",
                    "data:application/font-sfnt;base64",
                ]
                .iter()
                .any(|allowed| header.eq_ignore_ascii_case(allowed))
                {
                    continue;
                }
                if encoded.len() > 2 * 1024 * 1024 * 4 / 3 + 4 {
                    return Err("embedded SVG font exceeds 2 MiB".into());
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| "invalid embedded SVG font encoding")?;
                total += bytes.len();
                if total > 2 * 1024 * 1024 || fonts.faces().count() >= 16 {
                    return Err("embedded SVG fonts exceed the document limit".into());
                }
                let mut source = resvg::usvg::fontdb::Database::new();
                source.load_font_data(bytes);
                if source.faces().next().is_none() {
                    return Err("invalid embedded SVG font".into());
                }
                for face in source.faces() {
                    if fonts.faces().count() >= 16 {
                        return Err("embedded SVG fonts exceed the document limit".into());
                    }
                    let mut face = face.clone();
                    face.families = vec![(family.clone(), face.families[0].1)];
                    fonts.push_face_info(face);
                }
                break;
            }
        }
    }
    Ok(fonts)
}

fn font_face<'i>(
    parser: &mut cssparser::Parser<'i, '_>,
) -> Result<(Option<String>, Vec<String>), cssparser::ParseError<'i, ()>> {
    use cssparser::{Delimiter, Token};
    let mut family = None;
    let mut sources = Vec::new();
    while let Ok(token) = parser.next() {
        let Token::Ident(name) = token.clone() else {
            continue;
        };
        if parser.expect_colon().is_err() {
            continue;
        }
        let _: Result<(), cssparser::ParseError<'_, ()>> =
            parser.parse_until_before(Delimiter::Semicolon, |value| {
                if name.eq_ignore_ascii_case("font-family") {
                    let mut parts = Vec::new();
                    while let Ok(token) = value.next() {
                        match token {
                            Token::Ident(part) | Token::QuotedString(part) => {
                                parts.push(part.to_string())
                            }
                            _ => return Err(value.new_custom_error(())),
                        }
                    }
                    family = (!parts.is_empty()).then(|| parts.join(" "));
                } else if name.eq_ignore_ascii_case("src") {
                    while !value.is_exhausted() {
                        if let Ok(url) = value.try_parse(|p| p.expect_url()) {
                            sources.push(url.to_string());
                        } else {
                            value.next()?;
                        }
                    }
                }
                Ok(())
            });
    }
    Ok((family, sources))
}

struct Track {
    attribute: String,
    duration: f64,
    values: Vec<Value>,
    times: Vec<f64>,
    discrete: bool,
}

impl Track {
    fn parse(node: Node<'_, '_>) -> Result<Self, String> {
        if ["end", "repeatDur", "min", "max"]
            .iter()
            .any(|name| node.attribute(*name).is_some())
        {
            return Err("conditional SVG loop timing is unsupported".into());
        }
        if node.tag_name().name() != "animate"
            || node.attribute("href").is_some()
            || node
                .attribute(("http://www.w3.org/1999/xlink", "href"))
                .is_some()
        {
            return Err("SVG supports parent-targeted looping <animate> elements".into());
        }
        let attribute = node
            .attribute("attributeName")
            .ok_or("SVG animation has no attribute")?;
        if !matches!(
            attribute,
            "d" | "x"
                | "y"
                | "cx"
                | "cy"
                | "r"
                | "rx"
                | "ry"
                | "width"
                | "height"
                | "opacity"
                | "fill-opacity"
                | "stroke-opacity"
                | "stroke-width"
                | "fill"
                | "stroke"
        ) {
            return Err(format!("unsupported SVG animated attribute: {attribute}"));
        }
        let duration = clock(node.attribute("dur").unwrap_or_default())?;
        if !(0.1..=12.).contains(&duration) {
            return Err("SVG loop must be 0.1 to 12 seconds".into());
        }
        let begin = node.attribute("begin").unwrap_or("0s");
        let self_loop = node.attribute("id").is_some_and(|id| {
            begin
                .split(';')
                .map(str::trim)
                .eq(["0s", &format!("{id}.end")])
        });
        if !self_loop
            && (!(begin == "0s" || begin == "0")
                || node.attribute("repeatCount") != Some("indefinite"))
        {
            return Err("SVG supports zero-start indefinite or self-restarting loops".into());
        }
        if node.attribute("additive").is_some_and(|v| v != "replace")
            || node.attribute("accumulate").is_some_and(|v| v != "none")
        {
            return Err("additive SVG animation is unsupported".into());
        }
        let discrete = match node.attribute("calcMode").unwrap_or("linear") {
            "linear" => false,
            "discrete" => true,
            _ => return Err("SVG supports linear and discrete value animation".into()),
        };
        let raw = node
            .attribute("values")
            .map(|v| v.split(';').map(str::trim).collect::<Vec<_>>())
            .unwrap_or_else(|| {
                vec![
                    node.attribute("from").unwrap_or_default(),
                    node.attribute("to").unwrap_or_default(),
                ]
            });
        if !(2..=64).contains(&raw.len()) {
            return Err("SVG needs 2 to 64 keyframes".into());
        }
        let values = raw
            .iter()
            .map(|v| Value::parse(attribute, v))
            .collect::<Result<Vec<_>, _>>()?;
        let times = if let Some(times) = node.attribute("keyTimes") {
            times
                .split(';')
                .map(|v| v.trim().parse::<f64>().map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            (0..values.len())
                .map(|i| i as f64 / (values.len() - 1) as f64)
                .collect()
        };
        if times.len() != values.len()
            || times[0] != 0.
            || *times.last().unwrap() != 1.
            || times.iter().any(|v| !v.is_finite())
            || times.windows(2).any(|v| v[0] >= v[1])
        {
            return Err("invalid SVG keyTimes".into());
        }
        for values in values.windows(2) {
            values[0].interpolate(&values[1], 0.)?;
        }
        Ok(Self {
            attribute: attribute.into(),
            duration,
            values,
            times,
            discrete,
        })
    }

    fn value(&self, phase: f64) -> Result<String, String> {
        let index = self
            .times
            .partition_point(|t| *t <= phase)
            .saturating_sub(1)
            .min(self.times.len() - 2);
        let t = if self.discrete {
            0.
        } else {
            (phase - self.times[index]) / (self.times[index + 1] - self.times[index])
        };
        self.values[index].interpolate(&self.values[index + 1], t)
    }
}

fn clock(value: &str) -> Result<f64, String> {
    let (number, scale) = if let Some(v) = value.strip_suffix("ms") {
        (v, 0.001)
    } else {
        (value.strip_suffix('s').unwrap_or(value), 1.)
    };
    number
        .trim()
        .parse::<f64>()
        .map(|v| v * scale)
        .map_err(|e| e.to_string())
}

enum Value {
    Number(f64),
    Color(svgtypes::Color),
    Path(Vec<(u8, Vec<f64>)>),
}

impl Value {
    fn parse(attribute: &str, value: &str) -> Result<Self, String> {
        if attribute == "d" {
            use svgtypes::PathSegment as S;
            let segments = svgtypes::PathParser::from(value)
                .map(|segment| {
                    let segment = segment.map_err(|e| e.to_string())?;
                    let values = match segment {
                        S::MoveTo { x, y, .. }
                        | S::LineTo { x, y, .. }
                        | S::SmoothQuadratic { x, y, .. } => vec![x, y],
                        S::HorizontalLineTo { x, .. } => vec![x],
                        S::VerticalLineTo { y, .. } => vec![y],
                        S::CurveTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                            ..
                        } => vec![x1, y1, x2, y2, x, y],
                        S::SmoothCurveTo { x2, y2, x, y, .. } => vec![x2, y2, x, y],
                        S::Quadratic { x1, y1, x, y, .. } => vec![x1, y1, x, y],
                        S::ClosePath { .. } => vec![],
                        S::EllipticalArc { .. } => {
                            return Err("SVG arc animation is unsupported".into());
                        }
                    };
                    Ok((segment.command(), values))
                })
                .collect::<Result<Vec<_>, String>>()?;
            if segments.is_empty() {
                return Err("empty SVG animation path".into());
            }
            Ok(Self::Path(segments))
        } else if matches!(attribute, "fill" | "stroke") {
            value
                .parse()
                .map(Self::Color)
                .map_err(|e: svgtypes::Error| e.to_string())
        } else {
            let number = value.parse::<f64>().map_err(|e| e.to_string())?;
            if !number.is_finite() {
                return Err("invalid SVG animation number".into());
            }
            Ok(Self::Number(number))
        }
    }

    fn interpolate(&self, other: &Self, t: f64) -> Result<String, String> {
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        Ok(match (self, other) {
            (Self::Number(a), Self::Number(b)) => lerp(*a, *b).to_string(),
            (Self::Color(a), Self::Color(b)) => format!(
                "rgba({},{},{},{})",
                lerp(a.red.into(), b.red.into()).round() as u8,
                lerp(a.green.into(), b.green.into()).round() as u8,
                lerp(a.blue.into(), b.blue.into()).round() as u8,
                lerp(a.alpha.into(), b.alpha.into()) / 255.
            ),
            (Self::Path(a), Self::Path(b)) if a.len() == b.len() => {
                let mut output = String::new();
                for ((command, a), (other, b)) in a.iter().zip(b) {
                    if command != other || a.len() != b.len() {
                        return Err("SVG path keyframes must share commands".into());
                    }
                    output.push(*command as char);
                    for (a, b) in a.iter().zip(b) {
                        output.push_str(&format!("{} ", lerp(*a, *b)));
                    }
                }
                output
            }
            _ => return Err("incompatible SVG keyframes".into()),
        })
    }
}

fn snapshot(
    text: &str,
    nodes: &[Node<'_, '_>],
    tracks: &[Track],
    phase: f64,
) -> Result<String, String> {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for (node, track) in nodes.iter().zip(tracks) {
        let parent = node.parent_element().ok_or("SVG animation has no parent")?;
        if let Some(attr) = parent.attribute_node(track.attribute.as_str()) {
            edits.push((attr.range(), String::new()));
        }
        // Insert after the parsed tag name, preserving all original namespaces,
        // quoted attributes and embedded styles rather than reserializing XML.
        let start = parent.range().start + 1;
        let end = text[start..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|i| start + i)
            .ok_or("invalid SVG start tag")?;
        edits.push((
            end..end,
            format!(" {}=\"{}\"", track.attribute, track.value(phase)?),
        ));
        edits.push((node.range(), String::new()));
    }
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    if edits.windows(2).any(|e| e[0].0.end > e[1].0.start) {
        return Err("overlapping SVG animation targets".into());
    }
    let mut output = text.to_string();
    for (range, value) in edits.into_iter().rev() {
        output.replace_range(range, &value);
    }
    Ok(output)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const TYPING: &[u8] = br##"<!-- generator -->
    <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="160" height="30">
      <path id="line"><animate id="typing" attributeName="d" begin="0s;typing.end" dur="1s"
        values="m0,20 h0;m0,20 h160;m0,20 h160;m0,20 h0" keyTimes="0;0.6;0.85;1"/></path>
      <text font-family="monospace" font-size="20" fill="#ff5fa2"><textPath xlink:href="#line">ctty7</textPath></text>
    </svg>"##;

    pub(crate) fn typing_with_embedded_font() -> String {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(include_bytes!(
            "../../tests/fixtures/markdown/jetbrains-mono-ctty7.ttf"
        ));
        std::str::from_utf8(TYPING)
            .unwrap()
            .replace("font-family=\"monospace\"", "font-family=\"Typing fixture\"")
            .replace("<path", &format!("<style>@font-face {{font-family:'Typing fixture';src:url('data:font/ttf;base64,{encoded}')}}</style><path"))
    }

    #[test]
    fn markdown_svg_typing_renders_changing_pixels() {
        let image = render(typing_with_embedded_font().as_bytes())
            .unwrap()
            .expect("animated SVG must produce playback frames");
        assert!(image.frame_count() > 1);
        assert!(image.as_bytes(0).unwrap().iter().all(|v| *v == 0));
        assert!(
            image
                .as_bytes(image.frame_count() / 2)
                .unwrap()
                .iter()
                .any(|v| *v != 0)
        );
        assert_ne!(image.as_bytes(0), image.as_bytes(image.frame_count() / 2));
    }

    #[test]
    fn markdown_svg_preserves_static_images_and_bounds_animation() {
        assert!(
            render(br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"/>"#)
                .unwrap()
                .is_none()
        );
        let text = std::str::from_utf8(TYPING).unwrap();
        for (old, new) in [
            ("dur=\"1s\"", "dur=\"60s\""),
            ("width=\"160\"", "width=\"100000\""),
            ("0;0.6;0.85;1", "0;0.85;0.6;1"),
            ("m0,20 h160", "m0,20 v160"),
            ("0s;typing.end", "button.click"),
            ("attributeName=\"d\"", "attributeName=\"href\""),
        ] {
            assert!(render(text.replace(old, new).as_bytes()).is_err(), "{new}");
        }
    }

    #[test]
    fn markdown_svg_interpolates_numbers_colors_and_existing_attributes() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
          <rect width="20" height="20" fill="red" opacity="0">
            <animate attributeName="fill" values="#ff0000;#00ff00" dur="1s" repeatCount="indefinite"/>
            <animate attributeName="opacity" values="0;1" dur="1s" repeatCount="indefinite"/>
          </rect></svg>"##;
        let image = render(svg).unwrap().unwrap();
        assert_eq!(image.size(0), image.size(10));
        assert_ne!(image.as_bytes(0), image.as_bytes(10));
        assert_eq!(image.as_bytes(0).unwrap()[3], 0);
        assert!((120..=135).contains(&image.as_bytes(10).unwrap()[3]));
    }

    #[test]
    fn markdown_svg_embedded_font_matches_explicit_font_pixels() {
        let font = include_bytes!("../../tests/fixtures/markdown/jetbrains-mono-ctty7.ttf");
        let svg = typing_with_embedded_font();
        let doc = Document::parse(&svg).unwrap();
        let nodes: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("animate"))
            .collect();
        let tracks: Vec<_> = nodes.iter().map(|n| Track::parse(*n).unwrap()).collect();
        let snapshot = snapshot(&svg, &nodes, &tracks, 0.75).unwrap();
        let mut fonts = resvg::usvg::fontdb::Database::new();
        fonts.load_font_data(font.to_vec());
        let mut face = fonts.faces().next().unwrap().clone();
        face.families = vec![("Typing fixture".into(), face.families[0].1)];
        fonts.remove_face(face.id);
        fonts.push_face_info(face);
        let options = resvg::usvg::Options {
            fontdb: Arc::new(fonts),
            ..Default::default()
        };
        let tree = resvg::usvg::Tree::from_str(&snapshot, &options).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(160, 30).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let mut expected = pixmap.take();
        for pixel in expected.chunks_exact_mut(4) {
            gpui::swap_rgba_pa_to_bgra(pixel);
        }
        assert!(expected.iter().any(|v| *v != 0));
        let rendered = render(svg.as_bytes()).unwrap().unwrap();
        assert!(
            rendered.as_bytes(15).unwrap() == expected.as_slice(),
            "embedded font pixels differ from the explicit font reference"
        );
    }

    #[test]
    fn markdown_svg_fonts_are_document_scoped_and_ignore_external_urls() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(include_bytes!(
            "../../tests/fixtures/markdown/jetbrains-mono-ctty7.ttf"
        ));
        let svg = format!(
            "<svg><style>/* font */ @font-face {{font-family:Scoped Fixture;src:url(https://example.com/missing.ttf),url(data:font/truetype;base64,{encoded}) format('truetype')}}</style></svg>"
        );
        let doc = Document::parse(&svg).unwrap();
        let embedded = options(&doc).unwrap();
        assert!(
            embedded
                .fontdb
                .faces()
                .any(|f| f.families.iter().any(|(name, _)| name == "Scoped Fixture"))
        );
        let empty = Document::parse("<svg/>").unwrap();
        assert!(
            !options(&empty)
                .unwrap()
                .fontdb
                .faces()
                .any(|f| f.families.iter().any(|(name, _)| name == "Scoped Fixture"))
        );
        for src in [
            "https://example.com/missing.ttf",
            "file:///missing.ttf",
            "data:font/woff2;base64,AAAA",
        ] {
            let svg = format!(
                "<svg><style>@font-face {{font-family:Fixture;src:url('{src}')}}</style></svg>"
            );
            assert!(
                embedded_fonts(&Document::parse(&svg).unwrap())
                    .unwrap()
                    .faces()
                    .next()
                    .is_none()
            );
        }
        for src in [
            "data:font/ttf;base64,????".into(),
            format!("data:font/ttf;base64,{}", "A".repeat(3 * 1024 * 1024)),
        ] {
            let svg = format!(
                "<svg><style>@font-face {{font-family:Fixture;src:url('{src}')}}</style></svg>"
            );
            assert!(embedded_fonts(&Document::parse(&svg).unwrap()).is_err());
        }
    }
}

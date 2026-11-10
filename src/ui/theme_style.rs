//! The declarative chrome carried by imported Otty themes.
//!
//! Keep the original table hierarchy: sidebar tabs and horizontal tabs have
//! different overrides, and Windows material overrides are not macOS defaults.
use std::sync::{Arc, LazyLock};

use gpui::{App, BoxShadow, FontWeight, Global, Hsla, Styled, point, px, rgb};
use serde::{Deserialize, Serialize};

use crate::core::config::WindowBackdrop;

static DEFAULT_CONFIG: LazyLock<crate::core::config::Config> =
    LazyLock::new(crate::core::config::Config::default);

pub fn default_sidebar_width() -> f32 {
    DEFAULT_CONFIG.sidebar_width
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ThemeStyle(pub toml::Value);

#[derive(Default)]
pub struct ActiveStyle {
    pub profile: Option<Arc<ThemeStyle>>,
    pub ui_font: Option<String>,
    pub mono_font: Option<String>,
}

impl Global for ActiveStyle {}

pub fn active(cx: &App) -> Option<&ThemeStyle> {
    cx.try_global::<ActiveStyle>()?.profile.as_deref()
}

pub fn legible_palette(config: &crate::core::config::Config, styled: bool) -> bool {
    if styled {
        config.theme_style_legible_palette
    } else {
        config.theme_legible_palette
    }
}

impl ThemeStyle {
    pub fn validate(&self) -> Result<(), String> {
        fn walk(style: &ThemeStyle, value: &toml::Value, path: &str) -> Result<(), String> {
            if let Some(table) = value.as_table() {
                for (key, value) in table {
                    let path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    walk(style, value, &path)?;
                }
                return Ok(());
            }
            let key = path.rsplit('.').next().unwrap_or(path);
            if path.starts_with("meta.")
                || key.starts_with("font-") && key != "font-size" && key != "font-weight"
            {
                return Ok(());
            }
            if let Some(n) = value
                .as_float()
                .or_else(|| value.as_integer().map(|n| n as f64))
            {
                if !n.is_finite() || n < 0. || n > 4096. {
                    return Err(format!(
                        "{path}: expected a finite value between 0 and 4096"
                    ));
                }
                if key == "opacity" && n > 1. {
                    return Err(format!("{path}: opacity must be between 0 and 1"));
                }
            }
            if matches!(key, "margin" | "padding")
                && style.spacing(path).is_none_or(|a| {
                    a.iter()
                        .any(|n| !n.is_finite() || !(0.0..=4096.0).contains(n))
                })
            {
                return Err(format!("{path}: expected one to four finite lengths"));
            }
            if matches!(
                key,
                "background"
                    | "foreground"
                    | "surface"
                    | "color"
                    | "accent"
                    | "primary"
                    | "secondary"
                    | "tertiary"
                    | "success"
                    | "warning"
                    | "danger"
                    | "hover-bg"
                    | "active-bg"
                    | "cursor-text"
                    | "selection-background"
                    | "selection-foreground"
                    | "indicator-color"
                    | "text"
            ) {
                if let Some(color) = value.as_str() {
                    parse_color(color).map_err(|e| format!("{path}: {e}"))?;
                }
            }
            if key.starts_with("border-") || key == "border" && path != "panel.border" {
                if style.border(path, rgb(0).into()).is_none() {
                    return Err(format!(
                        "{path}: expected 'none' or '<width>px solid <color>'"
                    ));
                }
            }
            if key == "shadow" && style.shadow(path).is_none() {
                return Err(format!("{path}: unsupported shadow"));
            }
            if key == "material"
                && !matches!(
                    value.as_str(),
                    Some(
                        "none"
                            | "glass"
                            | "vibrancy-thin"
                            | "vibrancy-regular"
                            | "mica"
                            | "mica-alt"
                            | "acrylic"
                    )
                )
            {
                return Err(format!("{path}: unsupported material"));
            }
            Ok(())
        }
        walk(self, &self.0, "")
    }

    pub fn get(&self, path: &str) -> Option<&toml::Value> {
        path.split('.')
            .try_fold(&self.0, |value, key| value.get(key))
    }

    pub fn text(&self, path: &str) -> Option<&str> {
        self.get(path)?.as_str()
    }

    pub fn number(&self, path: &str) -> Option<f32> {
        let v = self.get(path)?;
        let n = v.as_float().or_else(|| v.as_integer().map(|n| n as f64))? as f32;
        n.is_finite().then_some(n)
    }

    pub fn color(&self, path: &str) -> Option<Hsla> {
        parse_color(self.text(path)?).ok()
    }

    pub fn packed(&self, path: &str) -> Option<u32> {
        let c = gpui::Rgba::from(self.color(path)?);
        Some(
            ((c.r * 255.).round() as u32) << 16
                | ((c.g * 255.).round() as u32) << 8
                | (c.b * 255.).round() as u32,
        )
    }

    pub fn spacing(&self, path: &str) -> Option<[f32; 4]> {
        if let Some(n) = self.number(path) {
            return Some([n; 4]);
        }
        let a = self.get(path)?.as_array()?;
        let a: Vec<f32> = a
            .iter()
            .map(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .map(|n| n as f32)
            .collect();
        match a.as_slice() {
            [v] => Some([*v; 4]),
            [v, h] => Some([*v, *h, *v, *h]),
            [t, h, b] => Some([*t, *h, *b, *h]),
            [t, r, b, l] => Some([*t, *r, *b, *l]),
            _ => None,
        }
    }

    pub fn weight(&self, path: &str) -> Option<FontWeight> {
        self.number(path).map(FontWeight)
    }

    /// Pick a face from the authored stack that actually exists on this OS.
    pub fn font(&self, path: &str, available: &[String]) -> Option<String> {
        let value = self.get(path)?;
        let names: Vec<&str> = match value {
            toml::Value::String(s) => vec![s],
            toml::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
            _ => return None,
        };
        names.into_iter().find_map(|name| {
            available
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(name))
                .cloned()
        })
    }

    pub fn terminal_color(&self, key: &str, fallback: Hsla) -> Hsla {
        if key == "background"
            && matches!(
                self.text("terminal.background"),
                Some("none" | "transparent")
            )
        {
            return fallback.alpha(0.);
        }
        self.color(&format!("terminal.{key}")).unwrap_or(fallback)
    }

    pub fn cursor_color(&self, shape: &str) -> Option<Hsla> {
        self.color(&format!("cursor.{shape}.color"))
            .or_else(|| self.color("cursor.color"))
            .or_else(|| self.color("terminal.cursor"))
            .or_else(|| self.color("terminal.foreground"))
    }

    pub fn cursor_text(&self, shape: &str) -> Option<Hsla> {
        self.color(&format!("cursor.{shape}.text"))
            .or_else(|| self.color("cursor.text"))
            .or_else(|| self.color("terminal.cursor-text"))
    }

    pub fn sidebar_material(&self) -> bool {
        cfg!(windows)
            && self
                .text("windows.sidebar.material")
                .is_some_and(|m| m != "none")
    }

    /// Windows declares its own backdrop. Other platforms use window/sidebar
    /// material, with ordinary transparency when no blur is requested.
    pub fn backdrop(&self) -> WindowBackdrop {
        let material = if cfg!(windows) {
            if self.sidebar_material() {
                self.text("windows.sidebar.material")
            } else {
                self.text("windows.material")
                    .or_else(|| self.text("window.material"))
                    .or_else(|| self.text("sidebar.material"))
            }
        } else {
            self.text("window.material")
                .or_else(|| self.text("sidebar.material"))
        };
        match material {
            Some("mica") => WindowBackdrop::Mica,
            Some("mica-alt") => WindowBackdrop::MicaAlt,
            Some("vibrancy-regular") => WindowBackdrop::Mica,
            Some("acrylic" | "glass" | "vibrancy-thin") => WindowBackdrop::Acrylic,
            _ => WindowBackdrop::Off,
        }
    }

    pub fn window_color(&self) -> Hsla {
        let declared = if cfg!(windows) {
            self.color("windows.color")
                .or_else(|| self.color("window.background"))
        } else {
            self.color("window.background")
        };
        declared.unwrap_or_else(|| {
            let material = if cfg!(windows) {
                self.text("windows.material")
                    .or_else(|| self.text("window.material"))
            } else {
                self.text("window.material")
            };
            if !self.sidebar_material() && material.is_some_and(|m| m != "none") {
                gpui::transparent_black()
            } else {
                self.color("terminal.background")
                    .or_else(|| self.color("panel.background"))
                    .unwrap_or_else(gpui::transparent_black)
            }
        })
    }

    pub fn sidebar_color(&self) -> Hsla {
        if self.sidebar_material() {
            let color = self
                .color("windows.sidebar.color")
                .unwrap_or_else(gpui::transparent_black);
            return color.opacity(self.number("windows.sidebar.opacity").unwrap_or(0.0));
        }
        self.color("sidebar.background").unwrap_or_else(|| {
            if self.text("sidebar.material").is_some_and(|m| m != "none") {
                gpui::transparent_black()
            } else {
                self.color("panel.background")
                    .unwrap_or_else(|| self.window_color())
            }
        })
    }

    pub fn details_color(&self) -> Hsla {
        self.color("details-panel.background")
            .unwrap_or_else(|| self.sidebar_color())
    }

    /// Apply explicit geometry and paint only; absent fields inherit Hitty's
    /// layout. This also serves user YAML, so duplicate/edit round trips retain
    /// the complete profile instead of flattening it to four colors.
    pub fn surface<E: Styled>(&self, mut el: E, path: &str, border: Hsla) -> E {
        if let Some(color) = self.color(&format!("{path}.background")) {
            el = el.bg(color);
        }
        if let Some(radius) = self.number(&format!("{path}.radius")) {
            el = el.rounded(px(radius.max(0.)));
        }
        if let Some((width, color)) = self.border(&format!("{path}.border"), border) {
            el = el.border(px(width)).border_color(color);
        }
        if let Some(shadows) = self.shadow(&format!("{path}.shadow")) {
            el = el.shadow(shadows);
        }
        if let Some([t, r, b, l]) = self.spacing(&format!("{path}.padding")) {
            el = el.pt(px(t)).pr(px(r)).pb(px(b)).pl(px(l));
        }
        for (edge, n) in ["top", "right", "bottom", "left"]
            .into_iter()
            .filter_map(|edge| {
                self.number(&format!("{path}.padding-{edge}"))
                    .map(|n| (edge, n))
            })
        {
            el = match edge {
                "top" => el.pt(px(n)),
                "right" => el.pr(px(n)),
                "bottom" => el.pb(px(n)),
                _ => el.pl(px(n)),
            };
        }
        if let Some(color) = self.color(&format!("{path}.foreground")) {
            el = el.text_color(color);
        }
        if let Some(weight) = self.weight(&format!("{path}.font-weight")) {
            el = el.font_weight(weight);
        }
        if let Some(size) = self.number(&format!("{path}.font-size")) {
            el = el.text_size(px(size));
        }
        if let Some(height) = self.number(&format!("{path}.height")) {
            el = el.h(px(height));
        }
        el
    }

    pub fn margin<E: Styled>(&self, el: E, path: &str) -> E {
        match self.spacing(path) {
            Some([t, r, b, l]) => el.mt(px(t)).mr(px(r)).mb(px(b)).ml(px(l)),
            None => el,
        }
    }

    pub fn border(&self, path: &str, fallback: Hsla) -> Option<(f32, Hsla)> {
        let text = self.text(path)?.trim();
        if text == "none" {
            return Some((0., gpui::transparent_black()));
        }
        let (width, rest) = text.split_once("px ")?;
        let color = rest.strip_prefix("solid ")?.trim();
        Some((
            width.parse().ok()?,
            if color == "auto" {
                fallback
            } else {
                parse_color(color).ok()?
            },
        ))
    }

    pub fn shadow(&self, path: &str) -> Option<Vec<BoxShadow>> {
        let text = self.text(path)?;
        if text == "none" {
            return Some(Vec::new());
        }
        let color_start = text.find("rgba(").or_else(|| text.find('#'))?;
        let color = parse_color(&text[color_start..]).ok()?;
        let values: Vec<f32> = text[..color_start]
            .split_whitespace()
            .map(|n| n.trim_end_matches("px").parse())
            .collect::<Result<_, _>>()
            .ok()?;
        let [x, y, blur, rest @ ..] = values.as_slice() else {
            return None;
        };
        Some(vec![BoxShadow {
            color,
            offset: point(px(*x), px(*y)),
            blur_radius: px(*blur),
            spread_radius: px(rest.first().copied().unwrap_or(0.)),
            inset: false,
        }])
    }

    pub fn tab<E: Styled>(
        &self,
        el: E,
        horizontal: bool,
        selected: bool,
        focused: bool,
        border: Hsla,
    ) -> E {
        let mut el = self.surface(el, "tab", border);
        if horizontal {
            el = self.surface(el, "tab-bar.tab", border);
        }
        if selected {
            el = self.surface(el, "tab.active", border);
            if horizontal {
                el = self.surface(el, "tab-bar.tab.active", border);
            }
            if !focused {
                el = self.surface(el, "tab.active.unfocused", border);
            }
        }
        el
    }

    pub fn tab_weight(&self, horizontal: bool, selected: bool) -> Option<FontWeight> {
        let active = selected
            .then(|| self.weight("tab.active.font-weight"))
            .flatten();
        let base = active.or_else(|| self.weight("tab.font-weight"));
        if horizontal {
            selected
                .then(|| self.weight("tab-bar.tab.active.font-weight"))
                .flatten()
                .or_else(|| self.weight("tab-bar.tab.font-weight"))
                .or(base)
        } else {
            base
        }
    }
}

pub fn typography(config: &crate::core::config::Config, cx: &App) -> (String, f32, f32) {
    let defaults = &*DEFAULT_CONFIG;
    let Some(style) = active(cx) else {
        return (
            config.font_family.clone(),
            config.font_size,
            config.line_height,
        );
    };
    let family = if config.font_family == defaults.font_family {
        cx.global::<ActiveStyle>()
            .mono_font
            .clone()
            .unwrap_or_else(|| config.font_family.clone())
    } else {
        config.font_family.clone()
    };
    let size = (style
        .number("token.font-size")
        .unwrap_or(defaults.font_size)
        + config.font_size
        - defaults.font_size)
        .clamp(6., 128.);
    let extra_height = style
        .text("token.adjust-cell-height")
        .and_then(|s| s.strip_suffix('%'))
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.)
        / 100.;
    (family, size, config.line_height * (1. + extra_height))
}

pub fn ui_font_size(config: &crate::core::config::Config, cx: &App) -> f32 {
    active(cx)
        .and_then(|s| s.number("token.font-size"))
        .map(|size| (size + config.ui_font_size - DEFAULT_CONFIG.ui_font_size).clamp(8., 64.))
        .unwrap_or(config.ui_font_size)
}

pub fn parse_color(text: &str) -> Result<Hsla, String> {
    let text = text.trim();
    if matches!(text, "none" | "transparent") {
        return Ok(gpui::transparent_black());
    }
    if let Some(hex) = text.strip_prefix('#') {
        let value = u32::from_str_radix(hex, 16).map_err(|_| format!("invalid color {text}"))?;
        return match hex.len() {
            6 => Ok(rgb(value).into()),
            8 => Ok(gpui::rgba(value).into()),
            _ => Err(format!("expected #RRGGBB or #RRGGBBAA: {text}")),
        };
    }
    let args = text
        .strip_prefix("rgba(")
        .or_else(|| text.strip_prefix("rgb("))
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| format!("unsupported color {text}"))?;
    let values: Vec<f32> = args
        .split(',')
        .map(|n| {
            n.trim()
                .parse::<f32>()
                .map_err(|_| format!("invalid color {text}"))
        })
        .collect::<Result<_, _>>()?;
    if !(3..=4).contains(&values.len())
        || values.iter().any(|n| !n.is_finite())
        || values[..3].iter().any(|n| !(0.0..=255.0).contains(n))
        || values.get(3).is_some_and(|a| !(0.0..=1.0).contains(a))
    {
        return Err(format!("out-of-range color {text}"));
    }
    Ok(gpui::Rgba {
        r: values[0] / 255.,
        g: values[1] / 255.,
        b: values[2] / 255.,
        a: values.get(3).copied().unwrap_or(1.),
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_alpha_geometry_and_shape_colors_survive_parsing() {
        let style: ThemeStyle = toml::from_str(
            r##"
            [sidebar]
            background = "#2528325d"
            border-right = "none"
            [container]
            margin = [4, 16, 16, 16]
            shadow = "0 1.5px 6px rgba(0,0,0,0.18)"
            [cursor]
            color = "#E5E9F0"
            [cursor.bar]
            color = "#88C0D0"
        "##,
        )
        .unwrap();
        assert!((style.color("sidebar.background").unwrap().a - 93. / 255.).abs() < 0.001);
        assert_eq!(style.spacing("container.margin"), Some([4., 16., 16., 16.]));
        let shadow = style.shadow("container.shadow").unwrap();
        assert_eq!(shadow[0].offset.y, px(1.5));
        assert_eq!(shadow[0].blur_radius, px(6.));
        assert_eq!(style.cursor_color("bar"), Some(rgb(0x88c0d0).into()));
        assert_eq!(style.cursor_color("block"), Some(rgb(0xe5e9f0).into()));
        assert_eq!(
            style
                .border("sidebar.border-right", rgb(0).into())
                .unwrap()
                .0,
            0.
        );
        for invalid in ["rgba(0,0,0,NaN)", "rgba(256,0,0,1)", "#12345", "red"] {
            assert!(parse_color(invalid).is_err(), "{invalid}");
        }
    }
}

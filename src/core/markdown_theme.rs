//! Local reading-theme packages. Document hosts never participate in loading
//! application configuration, including when the document itself is remote.

#[cfg(test)]
#[path = "markdown_github_tests.rs"]
mod github_tests;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Deserializer};
use serde_yaml::Value;

pub const DEFAULT_ID: &str = "github";
pub const DIRECTORY: &str = "markdown-themes";
pub const GITHUB: &str = include_str!("../../assets/markdown-themes/github.yaml");
pub const V1_UPGRADE_REQUIRED: &str = "This theme uses schema_version 1. Upgrade to v2; omitted styles will use GitHub defaults. The original file has not been changed.";
const MAX_THEME_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub [f32; 4]);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        parse_color(&value).map_err(serde::de::Error::custom)
    }
}

fn parse_color(value: &str) -> Result<Color, String> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            let n = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
            let rgba = if hex.len() == 6 { (n << 8) | 255 } else { n };
            return Ok(Color([
                ((rgba >> 24) & 255) as f32 / 255.,
                ((rgba >> 16) & 255) as f32 / 255.,
                ((rgba >> 8) & 255) as f32 / 255.,
                (rgba & 255) as f32 / 255.,
            ]));
        }
    } else if let Some(channels) = value
        .strip_prefix("rgba(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let channels: Result<Vec<f32>, _> = channels.split(',').map(|s| s.trim().parse()).collect();
        if let Ok(c) = channels {
            if c.len() == 4
                && c[..3]
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=255.).contains(v))
                && c[3].is_finite()
                && (0. ..=1.).contains(&c[3])
            {
                return Ok(Color([c[0] / 255., c[1] / 255., c[2] / 255., c[3]]));
            }
        }
    }
    Err(format!(
        "invalid color {value:?}; use #RRGGBB, #RRGGBBAA or rgba(r,g,b,a)"
    ))
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Typography {
    pub resolve_font_stack: bool,
    pub bold_weight: f32,
    pub heading_bottom_gap: Option<f32>,
    pub heading_padding: Option<[f32; 6]>,
    pub heading_borders: [bool; 6],
    pub code_line_height: Option<f32>,
    pub inline_code_size: Option<f32>,
    pub kbd_size: Option<f32>,
    #[serde(deserialize_with = "font_stack")]
    pub fonts: Vec<String>,
    #[serde(default, deserialize_with = "font_stack")]
    pub heading_fonts: Vec<String>,
    #[serde(deserialize_with = "font_stack")]
    pub code_fonts: Vec<String>,
    pub font_size: f32,
    pub code_size: f32,
    pub line_height: f32,
    pub heading_sizes: [f32; 6],
    pub heading_weights: [f32; 6],
    pub heading_line_height: f32,
    pub paragraph_gap: f32,
    pub heading_gap: f32,
}

fn font_stack<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    // serde_yaml accepts null as an empty sequence; v2 distinguishes an
    // explicit empty fallback list from an invalid null field.
    Option::<Vec<String>>::deserialize(deserializer)?
        .ok_or_else(|| serde::de::Error::custom("expected a font list, not null"))
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub paper_border_width: f32,
    pub code_border_width: f32,
    pub inline_code_radius: Option<f32>,
    pub inline_code_padding_x: f32,
    pub inline_code_padding_y: f32,
    /// Heading code padding in multiples of the heading font size.
    pub heading_code_padding_x_em: Option<f32>,
    pub heading_code_padding_y_em: Option<f32>,
    pub kbd_padding: f32,
    pub kbd_radius: f32,
    pub kbd_line_height: f32,
    pub table_radius: Option<f32>,
    pub table_gap: Option<f32>,
    pub table_fill: bool,
    pub table_padding_x: Option<f32>,
    pub table_padding_y: Option<f32>,
    pub rule_height: f32,
    pub rule_gap: Option<f32>,
    pub link_underline: LinkUnderline,
    pub roman_ordered_lists: bool,
    pub list_indent: Option<f32>,
    pub list_paragraph_gap: Option<f32>,
    pub alert_padding_y: Option<f32>,
    pub alert_padding_x: Option<f32>,
    pub alert_title_weight: f32,
    pub alert_title_line_height: Option<f32>,
    pub max_width: f32,
    pub compact_below: f32,
    pub outer_padding: f32,
    pub padding: f32,
    pub compact_padding: f32,
    pub radius: f32,
    pub shadow_blur: f32,
    pub shadow_offset: f32,
    pub code_padding: f32,
    pub code_radius: f32,
    pub quote_padding: f32,
    pub quote_padding_y: Option<f32>,
    pub quote_border: f32,
    pub quote_radius: f32,
    pub table_padding: f32,
    pub list_gap: f32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LinkUnderline {
    Always,
    Hover,
    Never,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DiffStyle {
    pub foreground: Color,
    pub background: Option<Color>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AlertPalette {
    pub border: Color,
    pub title: Color,
    pub icon: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Syntax {
    pub constant: Option<Color>,
    pub entity: Option<Color>,
    pub tag_name: Option<Color>,
    pub escape: Option<Color>,
    pub markup_heading: Option<Color>,
    pub markup_list: Option<Color>,
    pub link_uri: Option<Color>,
    pub diff_added: Option<DiffStyle>,
    pub diff_deleted: Option<DiffStyle>,
    pub diff_changed: Option<DiffStyle>,
    pub diff_hunk: Option<Color>,
    pub keyword: Color,
    pub number: Color,
    pub function: Color,
    pub variable: Color,
    pub variable_special: Color,
    pub r#type: Color,
    pub comment: Color,
    pub string: Color,
    pub tag: Color,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub heading_colors: Option<[Color; 6]>,
    pub heading_border: Option<Color>,
    pub rule: Option<Color>,
    pub inline_code_border: Option<Color>,
    pub kbd_background: Option<Color>,
    pub kbd_border: Option<Color>,
    pub kbd_shadow: Option<Color>,
    pub table_stripe: Option<Color>,
    pub table_row_border: Option<Color>,
    pub alerts: Option<[AlertPalette; 5]>,
    pub background: Color,
    pub paper: Color,
    pub paper_border: Color,
    pub shadow: Color,
    pub foreground: Color,
    pub heading: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub link: Color,
    pub link_hover: Color,
    pub selection: Color,
    pub inline_code_background: Color,
    pub inline_code_foreground: Color,
    pub code_background: Color,
    pub code_foreground: Color,
    pub code_border: Color,
    pub quote_background: Color,
    pub quote_foreground: Color,
    pub quote_border: Color,
    pub quote_link: Color,
    pub quote_code_background: Color,
    pub quote_code_foreground: Color,
    pub nested_quote_background: Color,
    pub nested_quote_border: Color,
    pub alert_background: Color,
    pub alert_foreground: Option<Color>,
    pub alert_border: Color,
    pub table_code_background: Color,
    pub table_hover: Option<Color>,
    pub syntax: Syntax,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    pub typography: Typography,
    pub layout: Layout,
    pub light: Palette,
    pub dark: Palette,
}

impl Theme {
    pub fn palette(&self, dark: bool) -> &Palette {
        if dark { &self.dark } else { &self.light }
    }

    fn same_style(&self, other: &Self) -> bool {
        self.typography == other.typography
            && self.layout == other.layout
            && self.light == other.light
            && self.dark == other.dark
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 2 {
            return Err(format!(
                "unsupported schema_version {}; expected 2",
                self.schema_version
            ));
        }
        if self.id.is_empty()
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        {
            return Err("id must contain lowercase ASCII letters, digits, '-' or '_'".into());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        let t = &self.typography;
        let l = &self.layout;
        for (key, value) in [
            ("typography.font_size", t.font_size),
            ("typography.code_size", t.code_size),
            ("typography.line_height", t.line_height),
            ("typography.heading_line_height", t.heading_line_height),
            ("layout.max_width", l.max_width),
            ("layout.compact_below", l.compact_below),
            ("layout.kbd_line_height", l.kbd_line_height),
        ] {
            if !value.is_finite() || value <= 0. {
                return Err(format!("{key} must be a finite positive number"));
            }
        }
        for value in t.heading_sizes {
            if !value.is_finite() || value <= 0. {
                return Err("typography.heading_sizes must contain six positive numbers".into());
            }
        }
        for (key, value) in [
            ("typography.bold_weight", t.bold_weight),
            ("layout.alert_title_weight", l.alert_title_weight),
        ] {
            if !value.is_finite() || !(100. ..=900.).contains(&value) {
                return Err(format!("{key} must be in 100..900"));
            }
        }
        for (key, value) in [
            ("typography.code_line_height", t.code_line_height),
            ("typography.inline_code_size", t.inline_code_size),
            ("typography.kbd_size", t.kbd_size),
            ("layout.alert_title_line_height", l.alert_title_line_height),
        ] {
            if value.is_some_and(|v| !v.is_finite() || v <= 0.) {
                return Err(format!("{key} must be a finite positive number"));
            }
        }
        for (key, value) in [
            ("typography.heading_bottom_gap", t.heading_bottom_gap),
            ("layout.inline_code_radius", l.inline_code_radius),
            (
                "layout.heading_code_padding_x_em",
                l.heading_code_padding_x_em,
            ),
            (
                "layout.heading_code_padding_y_em",
                l.heading_code_padding_y_em,
            ),
            ("layout.table_radius", l.table_radius),
            ("layout.table_gap", l.table_gap),
            ("layout.quote_padding_y", l.quote_padding_y),
            ("layout.table_padding_x", l.table_padding_x),
            ("layout.table_padding_y", l.table_padding_y),
            ("layout.rule_gap", l.rule_gap),
            ("layout.list_indent", l.list_indent),
            ("layout.list_paragraph_gap", l.list_paragraph_gap),
            ("layout.alert_padding_x", l.alert_padding_x),
            ("layout.alert_padding_y", l.alert_padding_y),
        ] {
            if value.is_some_and(|v| !v.is_finite() || v < 0.) {
                return Err(format!("{key} must be a finite nonnegative number"));
            }
        }
        if t.heading_padding
            .is_some_and(|values| values.into_iter().any(|v| !v.is_finite() || v < 0.))
        {
            return Err("typography.heading_padding must contain six nonnegative numbers".into());
        }
        for value in t.heading_weights {
            if !value.is_finite() || !(100. ..=900.).contains(&value) {
                return Err("typography.heading_weights must be in 100..900".into());
            }
        }
        for (key, value) in [
            ("typography.paragraph_gap", t.paragraph_gap),
            ("typography.heading_gap", t.heading_gap),
            ("layout.outer_padding", l.outer_padding),
            ("layout.padding", l.padding),
            ("layout.compact_padding", l.compact_padding),
            ("layout.radius", l.radius),
            ("layout.shadow_blur", l.shadow_blur),
            ("layout.shadow_offset", l.shadow_offset),
            ("layout.code_padding", l.code_padding),
            ("layout.code_radius", l.code_radius),
            ("layout.quote_padding", l.quote_padding),
            ("layout.quote_border", l.quote_border),
            ("layout.quote_radius", l.quote_radius),
            ("layout.table_padding", l.table_padding),
            ("layout.list_gap", l.list_gap),
            ("layout.paper_border_width", l.paper_border_width),
            ("layout.code_border_width", l.code_border_width),
            ("layout.inline_code_padding_x", l.inline_code_padding_x),
            ("layout.inline_code_padding_y", l.inline_code_padding_y),
            ("layout.kbd_padding", l.kbd_padding),
            ("layout.kbd_radius", l.kbd_radius),
            ("layout.rule_height", l.rule_height),
        ] {
            if !value.is_finite() || value < 0. {
                return Err(format!("{key} must be a finite nonnegative number"));
            }
        }
        Ok(())
    }
}

fn merge(base: &mut Value, overrides: Value) {
    match (base, overrides) {
        (Value::Mapping(base), Value::Mapping(overrides)) => {
            for (key, value) in overrides {
                if let Some(old) = base.get_mut(&key) {
                    merge(old, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overrides) => *base = overrides,
    }
}

pub fn parse(text: &str) -> Result<Theme, String> {
    let value: Value = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    let mapping = value.as_mapping().ok_or("theme must be a YAML mapping")?;
    if value["schema_version"].as_u64() == Some(1) {
        return Err(V1_UPGRADE_REQUIRED.into());
    }
    for key in ["schema_version", "id", "name", "light", "dark"] {
        if !mapping.contains_key(Value::String(key.into())) {
            return Err(format!("missing field {key}"));
        }
    }
    for key in ["light", "dark"] {
        if !value[key].is_mapping() {
            return Err(format!("{key} must be a mapping"));
        }
    }
    static BASE: OnceLock<Value> = OnceLock::new();
    let mut base = BASE
        .get_or_init(|| serde_yaml::from_str(GITHUB).expect("bundled theme YAML"))
        .clone();
    // Metadata is not inherited from the author of the default palette.
    for key in ["author", "description", "license"] {
        base[key] = Value::String(String::new());
    }
    merge(&mut base, value);
    let theme: Theme = serde_path_to_error::deserialize(base).map_err(|e| e.to_string())?;
    theme.validate()?;
    Ok(theme)
}

pub fn builtin() -> Arc<Theme> {
    static THEME: OnceLock<Arc<Theme>> = OnceLock::new();
    THEME
        .get_or_init(|| {
            let theme: Theme = serde_yaml::from_str(GITHUB).expect("complete bundled GitHub theme");
            theme.validate().expect("valid bundled GitHub theme");
            Arc::new(theme)
        })
        .clone()
}

pub fn themes_dir() -> Option<PathBuf> {
    crate::core::config::config_dir_path().map(|dir| dir.join(DIRECTORY))
}

pub fn open_directory() -> Result<PathBuf, String> {
    let dir = themes_dir().ok_or("configuration directory is unavailable")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn is_theme_path(path: &Path, directory: &Path) -> bool {
    path == directory
        || (path.parent() == Some(directory)
            && path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("yaml") || s.eq_ignore_ascii_case("yml")))
}

#[derive(Default)]
pub struct Snapshot {
    pub themes: BTreeMap<String, (PathBuf, Arc<Theme>)>,
    pub errors: Vec<(String, String)>,
}

pub fn scan(directory: Option<&Path>) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let Some(directory) = directory else {
        return snapshot;
    };
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return snapshot,
        Err(e) => {
            snapshot
                .errors
                .push((directory.display().to_string(), e.to_string()));
            return snapshot;
        }
    };
    let mut conflicts = BTreeSet::new();
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(e) => snapshot
                .errors
                .push((directory.display().to_string(), e.to_string())),
        }
    }
    paths.sort();
    for path in paths.into_iter().filter(|p| is_theme_path(p, directory)) {
        let result = (|| -> Result<Theme, String> {
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut text = String::new();
            file.take(MAX_THEME_BYTES + 1)
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
            if text.len() as u64 > MAX_THEME_BYTES {
                return Err("theme exceeds 256 KiB".into());
            }
            let theme = parse(&text)?;
            if theme.id == DEFAULT_ID {
                return Err(format!(
                    "id {:?} is reserved for a built-in theme",
                    theme.id
                ));
            }
            Ok(theme)
        })();
        match result {
            Ok(theme) => {
                if let Some((previous, _)) = snapshot.themes.get(&theme.id) {
                    snapshot.errors.push((
                        previous.display().to_string(),
                        format!("duplicate theme id {:?}", theme.id),
                    ));
                    conflicts.insert(theme.id.clone());
                }
                if conflicts.contains(&theme.id) {
                    snapshot.errors.push((
                        path.display().to_string(),
                        format!("duplicate theme id {:?}", theme.id),
                    ));
                }
                snapshot
                    .themes
                    .insert(theme.id.clone(), (path, Arc::new(theme)));
            }
            Err(error) => snapshot.errors.push((path.display().to_string(), error)),
        }
    }
    for id in conflicts {
        snapshot.themes.remove(&id);
    }
    snapshot.errors.sort();
    snapshot.errors.dedup();
    snapshot
}

#[derive(Clone)]
pub struct Entry {
    pub theme: Arc<Theme>,
    pub source: Option<PathBuf>,
    pub revision: u64,
}

pub struct Registry {
    pub entries: BTreeMap<String, Entry>,
    pub errors: Vec<(String, String)>,
    retained: Option<Entry>,
    revision: u64,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            entries: BTreeMap::from([(
                DEFAULT_ID.into(),
                Entry {
                    theme: builtin(),
                    source: None,
                    revision: 0,
                },
            )]),
            errors: Vec::new(),
            retained: None,
            revision: 0,
        }
    }
}

impl Registry {
    pub fn replace(&mut self, snapshot: Snapshot, selected: &str) {
        let previous = self.resolve(selected).clone();
        let mut entries = BTreeMap::from([(DEFAULT_ID.into(), self.entries[DEFAULT_ID].clone())]);
        for (id, (path, theme)) in snapshot.themes {
            let old = self
                .entries
                .get(&id)
                .or_else(|| self.retained.as_ref().filter(|e| e.theme.id == id));
            let revision = if let Some(old) = old.filter(|old| old.theme.same_style(&theme)) {
                old.revision
            } else {
                self.revision += 1;
                self.revision
            };
            entries.insert(
                id,
                Entry {
                    theme,
                    source: Some(path),
                    revision,
                },
            );
        }
        self.retained =
            (!entries.contains_key(selected) && previous.theme.id == selected).then_some(previous);
        self.entries = entries;
        self.errors = snapshot.errors;
        if self.unavailable(selected) && self.errors.is_empty() {
            self.errors.push((
                selected.to_owned(),
                format!(
                    "no theme package with id {selected:?} was found in the markdown-themes folder"
                ),
            ));
        }
    }

    pub fn resolve(&self, selected: &str) -> &Entry {
        self.entries
            .get(selected)
            .or_else(|| self.retained.as_ref().filter(|e| e.theme.id == selected))
            .unwrap_or(&self.entries[DEFAULT_ID])
    }

    pub fn unavailable(&self, selected: &str) -> bool {
        !self.entries.contains_key(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUSTOM: &str =
        "schema_version: 2\nid: study\nname: Study\nlight:\n  link: '#123456'\ndark: {}\n";

    #[test]
    fn v2_empty_variants_use_the_complete_github_style() {
        let custom =
            parse("schema_version: 2\nid: empty\nname: Empty\nlight: {}\ndark: {}\n").unwrap();
        let github = builtin();
        assert_eq!(custom.light, github.light);
        assert_eq!(custom.dark, github.dark);
        assert_eq!(custom.typography, github.typography);
        assert_eq!(custom.layout, github.layout);
        assert!(custom.author.is_empty());
        assert!(custom.license.is_empty());
    }

    #[test]
    fn v1_packages_are_untouched_and_recover_when_the_author_upgrades() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("study.yaml");
        let legacy = CUSTOM.replace("schema_version: 2", "schema_version: 1");
        std::fs::write(&path, &legacy).unwrap();
        let snapshot = scan(Some(dir.path()));
        assert!(snapshot.themes.is_empty());
        assert!(snapshot.errors[0].1.contains("Upgrade to v2"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), legacy);
        let mut registry = Registry::default();
        registry.replace(snapshot, "study");
        assert_eq!(registry.entries.len(), 1);
        assert_eq!(registry.resolve("study").theme.id, "github");
        std::fs::write(&path, CUSTOM).unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert_eq!(registry.resolve("study").theme.id, "study");
        let good = registry.resolve("study").clone();
        std::fs::write(&path, &legacy).unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert_eq!(registry.resolve("study").theme, good.theme);
        assert!(registry.unavailable("study"));
    }

    #[test]
    fn github_is_the_only_builtin_and_missing_custom_themes_fall_back() {
        let config: tty7_core::core::config::Config = serde_json::from_str("{}").unwrap();
        assert_eq!(config.markdown_theme, "github");
        let mut registry = Registry::default();
        assert_eq!(registry.entries.keys().collect::<Vec<_>>(), vec!["github"]);
        registry.replace(Snapshot::default(), "missing-theme");
        assert_eq!(registry.resolve("missing-theme").theme.id, "github");
        assert!(registry.errors.iter().any(|(id, _)| id == "missing-theme"));
    }

    #[test]
    fn markdown_package_overrides_only_explicit_fields_without_inheriting_authorship() {
        let theme = parse(CUSTOM).unwrap();
        let mut expected = builtin().light.clone();
        expected.link = parse_color("#123456").unwrap();
        assert_eq!(theme.light, expected);
        assert_eq!(theme.dark, builtin().dark);
        assert_eq!(theme.typography, builtin().typography);
        assert_eq!(theme.layout, builtin().layout);
        assert!(theme.author.is_empty());
    }

    #[test]
    fn v2_arrays_replace_defaults_and_null_is_an_explicit_override() {
        let source =
            format!("{CUSTOM}typography:\n  fonts: [Only]\nlayout:\n  table_radius: null\n");
        let source = source.replace(
            "light:\n  link: '#123456'",
            "light:\n  link: '#123456'\n  table_stripe: null",
        );
        let theme = parse(&source).unwrap();
        assert_eq!(theme.typography.fonts, vec!["Only"]);
        assert_eq!(theme.typography.code_fonts, builtin().typography.code_fonts);
        assert_eq!(theme.light.table_stripe, None);
        assert_eq!(theme.dark.table_stripe, builtin().dark.table_stripe);
        assert_eq!(theme.layout.table_radius, None);
        assert!(parse(&source.replace("fonts: [Only]", "fonts: null")).is_err());
    }

    #[test]
    fn markdown_package_rejects_invalid_fields_and_values() {
        for (yaml, expected) in [
            (
                CUSTOM.replace("schema_version: 2", "schema_version: 3"),
                "schema_version",
            ),
            (CUSTOM.replace("id: study", "id: Study"), "id"),
            (CUSTOM.replace("name: Study", "name: ''"), "name"),
            (CUSTOM.replace("dark: {}", ""), "dark"),
            (CUSTOM.replace("dark: {}", "dark: null"), "dark"),
            (CUSTOM.replace("link:", "lnik:"), "lnik"),
            (CUSTOM.replace("#123456", "#xyzxyz"), "color"),
            (
                CUSTOM.replace("dark: {}", "dark: {syntax: {number: bad}}"),
                "dark.syntax.number",
            ),
            (
                format!("{CUSTOM}typography:\n  font_size: -1\n"),
                "font_size",
            ),
            (format!("{CUSTOM}layout:\n  radius: -.inf\n"), "radius"),
        ] {
            let error = parse(&yaml).unwrap_err();
            assert!(error.contains(expected), "{expected}: {error}");
        }
        assert!(parse_color("rgba(256, 0, 0, 0.2)").is_err());
        assert!(parse_color("rgba(10, 0, 0, NaN)").is_err());
        assert_eq!(parse_color("#00000080").unwrap().0[3], 128. / 255.);
        assert_eq!(
            parse_color("rgba(255, 0, 128, 0.5)").unwrap().0,
            [1., 0., 128. / 255., 0.5]
        );
    }

    #[test]
    fn markdown_discovery_is_shallow_and_conflicts_exclude_all_sources() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested/hidden.yaml"), CUSTOM).unwrap();
        std::fs::write(dir.path().join("ignored.css"), CUSTOM).unwrap();
        std::fs::write(dir.path().join("a.YAML"), CUSTOM).unwrap();
        std::fs::write(dir.path().join("b.yml"), CUSTOM).unwrap();
        std::fs::write(dir.path().join("c.yaml"), CUSTOM).unwrap();
        std::fs::write(dir.path().join("bad.yml"), "not: a theme").unwrap();
        std::fs::write(
            dir.path().join("good.yml"),
            CUSTOM.replace("id: study", "id: other"),
        )
        .unwrap();
        std::fs::write(dir.path().join("reserved.yml"), GITHUB).unwrap();
        let snapshot = scan(Some(dir.path()));
        assert_eq!(snapshot.themes.keys().collect::<Vec<_>>(), vec!["other"]);
        assert_eq!(snapshot.errors.len(), 5);
        assert_eq!(
            snapshot
                .errors
                .iter()
                .filter(|(_, error)| error.contains("duplicate"))
                .count(),
            3
        );
    }

    #[test]
    fn markdown_hot_reload_retains_good_selection_and_recovers_without_reselecting() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("study.yaml");
        std::fs::write(&file, CUSTOM).unwrap();
        let mut registry = Registry::default();
        registry.replace(scan(Some(dir.path())), "study");
        let old = registry.resolve("study").clone();
        std::fs::write(&file, "broken: [").unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert!(registry.unavailable("study"));
        assert_eq!(registry.resolve("study").revision, old.revision);
        assert_eq!(registry.resolve("study").theme, old.theme);
        let mut startup = Registry::default();
        startup.replace(scan(Some(dir.path())), "study");
        assert_eq!(startup.resolve("study").theme.id, DEFAULT_ID);
        std::fs::write(&file, CUSTOM.replace("#123456", "#654321")).unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert!(!registry.unavailable("study"));
        assert!(registry.resolve("study").revision > old.revision);
        std::fs::remove_file(&file).unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert_eq!(
            registry.resolve("study").theme.light.link,
            parse_color("#654321").unwrap()
        );
        assert_eq!(registry.resolve("github").theme.id, "github");
    }

    #[test]
    fn markdown_missing_theme_reports_its_id_and_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::default();
        registry.replace(scan(Some(dir.path())), "study");
        assert_eq!(registry.resolve("study").theme.id, DEFAULT_ID);
        assert!(
            registry
                .errors
                .iter()
                .any(|(id, reason)| id == "study" && reason.contains("was found"))
        );
        std::fs::write(dir.path().join("study.yaml"), CUSTOM).unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert_eq!(registry.resolve("study").theme.id, "study");
        assert!(registry.errors.is_empty());
    }

    #[test]
    fn formerly_reserved_id_uses_missing_theme_fallback_and_recovers_when_installed() {
        let dir = tempfile::tempdir().unwrap();
        let selected = "paperglow";
        let mut registry = Registry::default();
        registry.replace(scan(Some(dir.path())), selected);
        assert!(registry.unavailable(selected));
        assert_eq!(registry.resolve(selected).theme.id, "github");
        assert!(registry.errors.iter().any(|(id, _)| id == selected));

        let package = dir.path().join("custom.yaml");
        std::fs::write(
            &package,
            CUSTOM.replace("id: study", &format!("id: {selected}")),
        )
        .unwrap();
        registry.replace(scan(Some(dir.path())), selected);
        assert!(!registry.unavailable(selected));
        let entry = registry.resolve(selected);
        assert_eq!(entry.theme.id, selected);
        assert_eq!(entry.source.as_ref(), Some(&package));
        assert_eq!(entry.theme.light.link, parse_color("#123456").unwrap());
        assert!(registry.errors.is_empty());
    }

    #[test]
    fn markdown_file_rename_and_metadata_edits_do_not_invalidate_rendering() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("before.yaml");
        std::fs::write(&file, CUSTOM).unwrap();
        let mut registry = Registry::default();
        registry.replace(scan(Some(dir.path())), "study");
        let revision = registry.resolve("study").revision;
        let renamed = dir.path().join("after.yml");
        std::fs::rename(file, &renamed).unwrap();
        std::fs::write(
            &renamed,
            CUSTOM.replace("name: Study", "name: Updated\nauthor: Author"),
        )
        .unwrap();
        registry.replace(scan(Some(dir.path())), "study");
        assert_eq!(registry.resolve("study").revision, revision);
        assert_eq!(registry.resolve("study").theme.name, "Updated");
        assert_eq!(registry.resolve("study").source.as_ref(), Some(&renamed));
        assert!(is_theme_path(dir.path(), dir.path()));
        assert!(!is_theme_path(
            &dir.path().join("nested/theme.yaml"),
            dir.path()
        ));
    }

    #[test]
    fn markdown_config_round_trip_keeps_selection_separate_from_application_theme() {
        use tty7_core::core::config::Config;
        let mut config: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(config.markdown_theme, DEFAULT_ID);
        let application_theme = config.theme.clone();
        config.markdown_theme = "study".into();
        let saved = serde_json::to_string(&config).unwrap();
        let loaded: Config = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.markdown_theme, "study");
        assert_eq!(loaded.theme, application_theme);
    }
}

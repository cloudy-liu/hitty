//! Source syntax uses bundled palettes over the active application background.

use crate::core::config::{
    Config, EDITOR_FONT_SIZE_DEFAULT, EDITOR_FONT_SIZE_MAX, EDITOR_FONT_SIZE_MIN,
    EDITOR_LINE_HEIGHT_DEFAULT, EDITOR_LINE_HEIGHT_MAX, EDITOR_LINE_HEIGHT_MIN,
};
use crate::ui::{
    app::Tty7App,
    presets::{ActiveAccent, match_wash_targets, wash},
};
use gpui::{App, Context, SharedString, Window, rgb};
use gpui_component::{ActiveTheme as _, ThemeMode, input::CodeEditorStyle};
use std::sync::{Arc, OnceLock};

struct Palettes {
    dark: Arc<CodeEditorStyle>,
    light: Arc<CodeEditorStyle>,
}

fn palettes() -> &'static Palettes {
    static PALETTES: OnceLock<Palettes> = OnceLock::new();
    PALETTES.get_or_init(|| {
        let dark: CodeEditorStyle = serde_json::from_str(include_str!(
            "../../assets/editor-themes/atom-one-dark.json"
        ))
        .expect("bundled Atom One Dark palette");
        let light: CodeEditorStyle =
            serde_json::from_str(include_str!("../../assets/editor-themes/github-light.json"))
                .expect("bundled GitHub Light palette");
        Palettes {
            dark: Arc::new(dark),
            light: Arc::new(light),
        }
    })
}

pub(crate) fn resolved_name(cx: &App) -> &'static str {
    if cx.theme().mode == ThemeMode::Dark {
        "Atom One Dark"
    } else {
        "GitHub Light"
    }
}

pub(crate) fn font_family(cx: &App) -> SharedString {
    let config = cx.global::<Config>();
    match config.editor_font_family.as_str() {
        "default" => cx.theme().mono_font_family.clone(),
        "terminal" => config.font_family.clone().into(),
        family => family.to_owned().into(),
    }
}

pub(crate) fn current(cx: &App) -> Arc<CodeEditorStyle> {
    let palettes = palettes();
    let mut style = if cx.theme().mode == ThemeMode::Dark {
        palettes.dark.clone()
    } else {
        palettes.light.clone()
    };
    let pack = |color| {
        let color = crate::terminal::palette::hsla_to_rgb(color);
        (color.r as u32) << 16 | (color.g as u32) << 8 | color.b as u32
    };
    let resolved = Arc::make_mut(&mut style);
    let colors = &mut Arc::make_mut(&mut resolved.highlight_theme).style;
    let background = cx.global::<crate::ui::presets::Surfaces>().window.base;
    colors.editor_background = Some(rgb(background).into());
    colors.editor_gutter_background = Some(rgb(background).into());
    let foreground = pack(colors.editor_foreground.expect("bundled editor foreground"));
    let (hit_target, active_target) = match_wash_targets(background, foreground);
    let accent = cx.global::<ActiveAccent>().0;
    // Recalculate search contrast against the actual application background.
    // Opaque fills keep the current match independent of the ordinary hit below it.
    resolved.search_match = rgb(wash(background, foreground, hit_target)).into();
    resolved.search_match_active = rgb(wash(background, accent, active_target)).into();
    style
}

pub(crate) fn language(language: &'static str) -> &'static str {
    use std::collections::HashMap;
    static ALIASES: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    let aliases = ALIASES.get_or_init(|| {
        let registry = gpui_component::highlighter::LanguageRegistry::singleton();
        let aliases: HashMap<_, _> = gpui_component::highlighter::Language::all()
            .map(|language| (language.name(), format!("tty7-{}", language.name())))
            .collect();
        let captures = regex::Regex::new(r"@([A-Za-z0-9_.-]+)").expect("capture regex");
        let syntax = &palettes().dark.highlight_theme.style.syntax;
        for (&name, alias) in &aliases {
            let mut config = registry.language(name).expect("bundled grammar");
            config.name = alias.clone().into();
            // The TSX crate ships only its TypeScript additions, so compose the
            // complete query before adding JSX roles.
            let query = if name == "tsx" {
                registry
                    .language("typescript")
                    .expect("TypeScript grammar")
                    .highlights
            } else if name == "cpp" {
                format!("{}\n{}", registry.language("c").expect("C grammar").highlights, config.highlights).into()
            } else {
                config.highlights.clone()
            };
            // This grammar represents negated type/membership tests as separate
            // tokens. The bundled composite-token patterns otherwise disable
            // Kotlin highlighting entirely.
            let query = if name == "kotlin" {
                query.replace("\"!is\"", "").replace("\"!in\"", "")
                    .replace("(string_literal\n\t\"$\" @punctuation.special\n\t(interpolated_identifier) @variable)", "(interpolated_identifier) @variable")
                    .replace("(string_literal\n\t\"${\" @punctuation.special\n\t(interpolated_expression)\n\t\"}\" @punctuation.special)", "")
            } else {
                query.to_string()
            };
            // This highlighter keeps the first capture of an identical range.
            // Place specific editor rules before the grammar's broad rules.
            let query = if name == "python" {
                let start = query.find("; Builtin functions").expect("Python builtin section");
                let end = query.find("; Function definitions").expect("Python definitions section");
                format!("{}\n{}\n{}{}", &query[start..end], extra_highlights(name), &query[..start], &query[end..])
            } else if name == "rust" {
                // Exact subroles retain their parent style in One Dark while
                // GitHub Light distinguishes references, fields and variants.
                let query = query
                    .replace("@constructor", "@constructor.variant")
                    // Macro token trees are re-parsed by the Rust injection;
                    // a broad outer capture would mask its called constructor.
                    .replace("((identifier) @constructor.variant", "([(_expression/identifier) (_pattern/identifier)] @constructor.variant")
                    .replace("@property", "@property.rust")
                    .replace("\"&\" @operator", "\"&\" @operator.reference");
                let variables = extra_highlights(name).replace("@variable", "@variable.rust");
                format!("{}\n{}\n{}\n{}", include_str!("../../assets/editor-themes/rust-imports.scm"), include_str!("../../assets/editor-themes/rust-calls.scm"), query, variables)
            } else {
                format!("{}\n{}", extra_highlights(name), query)
            };
            config.highlights = captures
                .replace_all(&query, |capture: &regex::Captures| {
                    let role = &capture[1];
                    let qualified = format!("{name}.{role}");
                    if syntax.captures.contains_key(&qualified) {
                        format!("@{qualified}")
                    } else {
                        capture[0].to_owned()
                    }
                })
                .into_owned()
                .into();
            if name == "markdown" {
                // Fence labels are captured dynamically; bind their canonical
                // language names to the editor-only registry entries as well.
                let fenced = "(fenced_code_block\n  (info_string\n    (language) @injection.language)\n  (code_fence_content) @injection.content)";
                assert!(config.injections.contains(fenced), "bundled Markdown fence query");
                let mut fences = String::new();
                for (original, target) in &aliases {
                    fences.push_str(&format!("((fenced_code_block (info_string (language) @__fence_language) (code_fence_content) @injection.content) (#eq? @__fence_language \"{original}\") (#set! injection.language \"{target}\"))\n"));
                }
                config.injections = config.injections.replacen(fenced, &fences, 1).into();
                config.injection_languages = aliases.values().map(|alias| alias.clone().into()).collect();
            }
            for (original, target) in &aliases {
                config.injections = config
                    .injections
                    .replace(&format!("#set! injection.language \"{original}\""), &format!("#set! injection.language \"{target}\""))
                    .into();
            }
            for language in &mut config.injection_languages {
                if let Some(target) = aliases.get(language.as_str()) {
                    *language = target.clone().into();
                }
            }
            registry.register(alias, &config);
        }
        aliases
    });
    aliases
        .get(language)
        .map(String::as_str)
        .unwrap_or(language)
}

fn extra_highlights(language: &str) -> &'static str {
    match language {
        "rust" => include_str!("../../assets/editor-themes/rust-highlights.scm"),
        "python" => include_str!("../../assets/editor-themes/python-highlights.scm"),
        "javascript" => concat!(
            include_str!("../../assets/editor-themes/javascript-highlights.scm"),
            include_str!("../../assets/editor-themes/jsx-highlights.scm"),
            "\n(formal_parameters (identifier) @variable.parameter)\n"
        ),
        "typescript" => include_str!("../../assets/editor-themes/javascript-highlights.scm"),
        "tsx" => concat!(
            include_str!("../../assets/editor-themes/javascript-highlights.scm"),
            include_str!("../../assets/editor-themes/jsx-highlights.scm")
        ),
        "css" => include_str!("../../assets/editor-themes/css-highlights.scm"),
        "c" | "cpp" => "\n(primitive_type) @type.builtin\n",
        "java" => {
            "\n(formal_parameter name: (identifier) @variable.parameter)\n(argument_list (identifier) @variable.argument)\n"
        }
        "diff" => "\n(addition) @markup.inserted\n(deletion) @markup.deleted\n",
        "bash" => {
            "\n((command_name) @function.builtin (#match? @function.builtin \"^(echo|printf|cd|pwd|read|export|set|unset|source|alias|type|test|true|false|exit|shift|eval|exec|trap|umask|wait|jobs|fg|bg)$\"))\n"
        }
        "ruby" => {
            "\n(assignment left: (identifier) @variable.definition)\n((call !receiver method: (identifier) @function.builtin) (#match? @function.builtin \"^(puts|p|print|printf|warn|require|require_relative|load|raise|fail|loop|sleep|rand|exit|abort|system|exec|eval|binding)$\"))\n"
        }
        "markdown" => {
            "\n[(atx_h1_marker) (atx_h2_marker) (atx_h3_marker) (atx_h4_marker) (atx_h5_marker) (atx_h6_marker)] @punctuation.heading\n(setext_heading (paragraph) @title.setext)\n"
        }
        "sql" => {
            "\n((literal) @number (#match? @number \"^[-+]?[0-9]+$\"))\n((literal) @float (#match? @float \"^[-+]?[0-9]*\\\\.[0-9]+$\"))\n"
        }
        "lua" => "\n(parameters (identifier) @variable.parameter)\n",
        "make" => "\n(variable_assignment name: (word) @variable)\n(targets (word) @function)\n",
        _ => "",
    }
}

impl Tty7App {
    pub(crate) fn change_focused_font_size(
        &mut self,
        delta: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_source_has_focus(window, cx) {
            self.set_editor_font_size(cx.global::<Config>().editor_font_size + delta, cx);
        } else {
            self.change_font_size(delta, cx);
        }
    }

    pub(crate) fn reset_focused_font_size(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.editor_source_has_focus(window, cx) {
            self.set_editor_font_size(EDITOR_FONT_SIZE_DEFAULT, cx);
        } else {
            self.reset_font_size(cx);
        }
    }
    pub(crate) fn set_editor_font_family(&mut self, family: String, cx: &mut Context<Self>) {
        if !family.trim().is_empty() {
            self.update_config(cx, |config| {
                config.editor_font_family = family.trim().into()
            });
            cx.refresh_windows();
        }
    }

    pub(crate) fn set_editor_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        let size = if size.is_finite() {
            size.clamp(EDITOR_FONT_SIZE_MIN, EDITOR_FONT_SIZE_MAX)
        } else {
            EDITOR_FONT_SIZE_DEFAULT
        };
        self.update_config(cx, |config| config.editor_font_size = size);
        cx.refresh_windows();
    }

    pub(crate) fn set_editor_line_height(&mut self, height: f32, cx: &mut Context<Self>) {
        let height = if height.is_finite() {
            height.clamp(EDITOR_LINE_HEIGHT_MIN, EDITOR_LINE_HEIGHT_MAX)
        } else {
            EDITOR_LINE_HEIGHT_DEFAULT
        };
        self.update_config(cx, |config| config.editor_line_height = height);
        cx.refresh_windows();
    }
}

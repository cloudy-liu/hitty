use super::*;

#[test]
fn github_defaults_match_live_site_layout_and_link_behavior() {
    let theme = builtin();
    let live: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/github-live-2026-10-05/light.json"
    ))
    .unwrap();
    let body = &live["styles"][0];
    let content_width: f32 = body["maxWidth"]
        .as_str()
        .unwrap()
        .trim_end_matches("px")
        .parse()
        .unwrap();
    let padding: f32 = live["parents"][1]["padding"]
        .as_str()
        .unwrap()
        .trim_end_matches("px")
        .parse()
        .unwrap();
    // The native card includes padding; GitHub's article content cap excludes it.
    assert_eq!(
        theme.layout.max_width - 2. * theme.layout.padding,
        content_width
    );
    assert_eq!(theme.layout.padding, padding);
    assert_eq!(theme.layout.compact_padding, padding);
    let link = live["styles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["selector"] == "article p a")
        .unwrap();
    assert_eq!(link["textDecorationLine"], "underline");
    assert_eq!(theme.layout.link_underline, LinkUnderline::Always);
    let heading_code = live["styles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["selector"] == "article h2 code")
        .unwrap();
    let size = theme.typography.heading_sizes[1];
    for (actual, property) in [
        (
            theme.layout.heading_code_padding_x_em.unwrap() * size,
            "paddingLeft",
        ),
        (
            theme.layout.heading_code_padding_y_em.unwrap() * size,
            "paddingTop",
        ),
    ] {
        let expected: f32 = heading_code[property]
            .as_str()
            .unwrap()
            .trim_end_matches("px")
            .parse()
            .unwrap();
        assert!((actual - expected).abs() < 0.001);
    }
}

// Read the last applicable declaration, including comma-separated selectors.
// This snapshot has no media-query-dependent values in the selectors below.
fn css<'a>(source: &'a str, selector: &str, property: &str) -> &'a str {
    source
        .split('}')
        .filter_map(|block| block.split_once('{'))
        .filter(|(selectors, _)| {
            selectors
                .split(',')
                .any(|s| s.rsplit("*/").next().unwrap().trim() == selector)
        })
        .flat_map(|(_, declarations)| declarations.split(';'))
        .filter_map(|declaration| declaration.trim().split_once(':'))
        .filter(|(name, _)| name.trim() == property)
        .map(|(_, value)| value.trim())
        .last()
        .unwrap_or_else(|| panic!("missing {selector} {property}"))
}

fn dimension(value: &str, em: f32) -> f32 {
    if let Some(value) = value.strip_suffix("rem") {
        value.parse::<f32>().unwrap() * 16.
    } else if let Some(value) = value.strip_suffix("em") {
        value.parse::<f32>().unwrap() * em
    } else if let Some(value) = value.strip_suffix('%') {
        value.parse::<f32>().unwrap() * em / 100.
    } else {
        value.trim_end_matches("px").parse().unwrap()
    }
}

#[test]
fn github_reading_palette_and_typography_match_the_upstream_css_snapshot() {
    let theme = parse(GITHUB).unwrap();
    for (dark, source) in [
        (
            false,
            include_str!(
                "../../tests/fixtures/github-markdown-css-5.9.0/github-markdown-light.css"
            ),
        ),
        (
            true,
            include_str!("../../tests/fixtures/github-markdown-css-5.9.0/github-markdown-dark.css"),
        ),
    ] {
        let p = theme.palette(dark);
        for (actual, selector, property) in [
            (p.background, ".markdown-body", "background-color"),
            (p.paper, ".markdown-body", "background-color"),
            (p.foreground, ".markdown-body", "color"),
            (p.heading, ".markdown-body", "color"),
            (p.link, ".markdown-body a", "color"),
            (p.link_hover, ".markdown-body a", "color"),
            (p.muted, ".markdown-body h6", "color"),
            (p.quote_foreground, ".markdown-body blockquote", "color"),
            (
                p.inline_code_background,
                ".markdown-body code",
                "background-color",
            ),
            (p.inline_code_foreground, ".markdown-body", "color"),
            (p.code_background, ".markdown-body pre", "background-color"),
            (p.code_foreground, ".markdown-body pre", "color"),
            (
                p.table_stripe.unwrap(),
                ".markdown-body table tr:nth-child(2n)",
                "background-color",
            ),
            (
                p.kbd_background.unwrap(),
                ".markdown-body kbd",
                "background-color",
            ),
            (p.syntax.keyword, ".markdown-body .pl-k", "color"),
            (p.syntax.number, ".markdown-body .pl-c1", "color"),
            (p.syntax.constant.unwrap(), ".markdown-body .pl-c1", "color"),
            (p.syntax.function, ".markdown-body .pl-en", "color"),
            (p.syntax.entity.unwrap(), ".markdown-body .pl-e", "color"),
            (p.syntax.variable, ".markdown-body .pl-smi", "color"),
            (p.syntax.variable_special, ".markdown-body .pl-v", "color"),
            (p.syntax.r#type, ".markdown-body .pl-e", "color"),
            (p.syntax.comment, ".markdown-body .pl-c", "color"),
            (p.syntax.string, ".markdown-body .pl-s", "color"),
            (
                p.syntax.tag_name.unwrap(),
                ".markdown-body .pl-ent",
                "color",
            ),
            (
                p.syntax.escape.unwrap(),
                ".markdown-body .pl-sr .pl-cce",
                "color",
            ),
            (
                p.syntax.markup_heading.unwrap(),
                ".markdown-body .pl-mh",
                "color",
            ),
            (
                p.syntax.markup_list.unwrap(),
                ".markdown-body .pl-ml",
                "color",
            ),
            (
                p.syntax.link_uri.unwrap(),
                ".markdown-body .pl-corl",
                "color",
            ),
            (
                p.syntax.diff_hunk.unwrap(),
                ".markdown-body .pl-mdr",
                "color",
            ),
        ] {
            assert_eq!(
                actual,
                parse_color(css(source, selector, property)).unwrap(),
                "{dark} {selector} {property}"
            );
        }
        for (actual, selector, property) in [
            (p.border, ".markdown-body table td", "border"),
            (p.quote_border, ".markdown-body blockquote", "border-left"),
            (
                p.heading_border.unwrap(),
                ".markdown-body h1",
                "border-bottom",
            ),
            (
                p.heading_border.unwrap(),
                ".markdown-body h2",
                "border-bottom",
            ),
        ] {
            assert_eq!(
                actual,
                parse_color(
                    css(source, selector, property)
                        .split_whitespace()
                        .last()
                        .unwrap()
                )
                .unwrap()
            );
        }
        assert_eq!(
            p.rule.unwrap(),
            parse_color(css(source, ".markdown-body hr", "background-color")).unwrap()
        );
        assert_eq!(p.kbd_border, p.heading_border); // Unexpanded --borderColor-muted in kbd.
        assert_eq!(p.kbd_shadow, p.heading_border);
        for (style, selector) in [
            (
                p.syntax.diff_added.as_ref().unwrap(),
                ".markdown-body .pl-mi1",
            ),
            (
                p.syntax.diff_deleted.as_ref().unwrap(),
                ".markdown-body .pl-md",
            ),
            (
                p.syntax.diff_changed.as_ref().unwrap(),
                ".markdown-body .pl-mc",
            ),
        ] {
            assert_eq!(
                style.foreground,
                parse_color(css(source, selector, "color")).unwrap()
            );
            assert_eq!(
                style.background.unwrap(),
                parse_color(css(source, selector, "background-color")).unwrap()
            );
        }
        for (ix, kind) in ["note", "tip", "important", "warning", "caution"]
            .iter()
            .enumerate()
        {
            let alert = &p.alerts.as_ref().unwrap()[ix];
            let selector = format!(".markdown-body .markdown-alert.markdown-alert-{kind}");
            assert_eq!(
                alert.border,
                parse_color(css(source, &selector, "border-left-color")).unwrap()
            );
            let selector = format!("{selector} .markdown-alert-title");
            assert_eq!(
                alert.title,
                parse_color(css(source, &selector, "color")).unwrap()
            );
        }
        let t = &theme.typography;
        let l = &theme.layout;
        for (actual, selector, property, em) in [
            (t.font_size, ".markdown-body", "font-size", 16.),
            (t.line_height, ".markdown-body", "line-height", 1.),
            (t.bold_weight, ".markdown-body strong", "font-weight", 1.),
            (t.code_size, ".markdown-body pre", "font-size", 16.),
            (
                t.code_line_height.unwrap(),
                ".markdown-body pre",
                "line-height",
                1.,
            ),
            (t.paragraph_gap, ".markdown-body p", "margin-bottom", 16.),
            (t.heading_gap, ".markdown-body h1", "margin-top", 16.),
            (
                t.heading_bottom_gap.unwrap(),
                ".markdown-body h1",
                "margin-bottom",
                16.,
            ),
            (l.code_padding, ".markdown-body pre", "padding", 16.),
            (l.code_radius, ".markdown-body pre", "border-radius", 16.),
            (
                l.inline_code_radius.unwrap(),
                ".markdown-body code",
                "border-radius",
                16.,
            ),
            (l.rule_height, ".markdown-body hr", "height", 16.),
            (l.kbd_line_height, ".markdown-body kbd", "line-height", 16.),
            (l.kbd_padding, ".markdown-body kbd", "padding", 16.),
            (l.kbd_radius, ".markdown-body kbd", "border-radius", 16.),
            (
                l.list_indent.unwrap(),
                ".markdown-body ul",
                "padding-left",
                16.,
            ),
        ] {
            assert!(
                (actual - dimension(css(source, selector, property), em)).abs() < 0.001,
                "{dark} {selector} {property}"
            );
        }
        for ix in 0..6 {
            let selector = format!(".markdown-body h{}", ix + 1);
            assert!(
                (t.heading_sizes[ix] - dimension(css(source, &selector, "font-size"), 16.)).abs()
                    < 0.001
            );
            assert_eq!(
                t.heading_weights[ix],
                dimension(css(source, &selector, "font-weight"), 1.)
            );
            assert_eq!(
                t.heading_line_height,
                dimension(css(source, &selector, "line-height"), 1.)
            );
        }
        for (ix, selector) in [".markdown-body h1", ".markdown-body h2"]
            .iter()
            .enumerate()
        {
            assert!(
                (t.heading_padding.unwrap()[ix]
                    - dimension(css(source, selector, "padding-bottom"), t.heading_sizes[ix]))
                .abs()
                    < 0.001
            );
        }
        let padding = css(source, ".markdown-body code", "padding")
            .split_whitespace()
            .collect::<Vec<_>>();
        assert!((l.inline_code_padding_y - dimension(padding[0], t.code_size)).abs() < 0.001);
        assert!((l.inline_code_padding_x - dimension(padding[1], t.code_size)).abs() < 0.001);
        let padding = css(source, ".markdown-body table td", "padding")
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(l.table_padding_y.unwrap(), dimension(padding[0], 16.));
        assert_eq!(l.table_padding_x.unwrap(), dimension(padding[1], 16.));
        assert_eq!(
            l.rule_gap.unwrap(),
            dimension(
                css(source, ".markdown-body hr", "margin")
                    .split_whitespace()
                    .next()
                    .unwrap(),
                16.
            )
        );
        assert_eq!(css(source, ".markdown-body a", "text-decoration"), "none");
        assert_eq!(
            css(source, ".markdown-body a:hover", "text-decoration"),
            "underline"
        );
    }
}

#[test]
fn new_theme_fields_reject_invalid_input_and_only_the_builtin_id_is_reserved() {
    let base = "schema_version: 2\nid: custom\nname: Custom\nlight: {}\ndark: {}\n";
    for (section, field, value) in [
        ("typography", "bold_weight", "901"),
        ("typography", "heading_bottom_gap", "-1"),
        ("typography", "code_line_height", "0"),
        ("typography", "inline_code_size", ".inf"),
        ("layout", "rule_height", "-1"),
        ("layout", "kbd_line_height", "0"),
        ("layout", "link_underline", "sometimes"),
        ("layout", "heading_code_padding_x_em", "-1"),
        ("layout", "heading_code_padding_y_em", ".inf"),
        ("layout", "table_fill", "42"),
        ("light", "kbd_border", "bad"),
    ] {
        let yaml = if section == "light" {
            base.replace("light: {}", &format!("light:\n  {field}: {value}"))
        } else {
            format!("{base}{section}:\n  {field}: {value}\n")
        };
        let error = parse(&yaml).unwrap_err();
        assert!(error.contains(field), "{field}: {error}");
    }
    let dir = tempfile::tempdir().unwrap();
    for name in ["github", "paperglow"] {
        std::fs::write(
            dir.path().join(format!("{name}.yaml")),
            base.replace("id: custom", &format!("id: {name}")),
        )
        .unwrap();
    }
    let snapshot = scan(Some(dir.path()));
    assert_eq!(snapshot.themes.len(), 1);
    assert_eq!(snapshot.errors.len(), 1);
    assert!(
        snapshot
            .errors
            .iter()
            .all(|(_, reason)| reason.contains("reserved"))
    );
    let mut registry = Registry::default();
    registry.replace(Snapshot::default(), "paperglow");
    assert!(registry.unavailable("paperglow"));
    assert_eq!(registry.resolve("paperglow").theme.id, "github");
    registry.replace(snapshot, "paperglow");
    assert!(!registry.unavailable("paperglow"));
    assert_eq!(registry.resolve("paperglow").theme.id, "paperglow");
    assert!(registry.resolve("paperglow").source.is_some());
    assert_eq!(registry.entries.len(), 2);
    assert_eq!(registry.resolve("github").theme, builtin());
}

#[test]
fn github_selection_and_checked_controls_match_pinned_primer_tokens() {
    let selection = include_str!("../../tests/fixtures/primer/selection.json5");
    let background = include_str!("../../tests/fixtures/primer/bgColor.json5");
    let control = include_str!("../../tests/fixtures/primer/control.json5");
    assert!(selection.contains("$value: '{bgColor.accent.emphasis}'"));
    assert!(
        background
            .split("accent: {")
            .nth(1)
            .unwrap()
            .split("emphasis: {")
            .nth(1)
            .unwrap()
            .contains("$value: '{base.color.blue.5}'")
    );
    assert!(
        control
            .split("checked: {")
            .nth(1)
            .unwrap()
            .contains("$value: '{bgColor.accent.emphasis}'")
    );
    let alphas = regex::Regex::new(r"alpha:\s*([0-9.]+)")
        .unwrap()
        .captures_iter(selection)
        .map(|capture| capture[1].parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    let theme = builtin();
    for (dark, source, alpha) in [
        (
            false,
            include_str!("../../tests/fixtures/primer/light.json5"),
            alphas[1],
        ),
        (
            true,
            include_str!("../../tests/fixtures/primer/dark.json5"),
            alphas[0],
        ),
    ] {
        let hex = source
            .split("blue: {")
            .nth(1)
            .unwrap()
            .split("'5': {")
            .nth(1)
            .unwrap()
            .split("hex: '")
            .nth(1)
            .unwrap()
            .split('\'')
            .next()
            .unwrap();
        let accent = parse_color(hex).unwrap();
        assert_eq!(theme.palette(dark).accent, accent);
        let mut selected = accent;
        selected.0[3] = alpha;
        assert_eq!(theme.palette(dark).selection, selected);
    }
}

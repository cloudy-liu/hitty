use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{
    Anchor, AnyElement, App, Context, ImageSource, RenderImage, div, img, prelude::*, px, rgb, svg,
};
use gpui_component::button::Button;
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use image::{Frame, ImageBuffer, Rgba, RgbaImage, imageops::FilterType};
use resvg::{tiny_skia, usvg};
use smallvec::SmallVec;

use crate::core::{cli_agent::CLIAgent, config::Config};
use crate::ui::app::HittyApp;
use crate::ui::i18n::{L10nKey, t, t_fmt};
use crate::ui::presets::AgentIcons;

const STANDARD_GLYPH_RATIO: f32 = 0.78;

const FIRST: [CLIAgent; 5] = [
    CLIAgent::Claude,
    CLIAgent::Codex,
    CLIAgent::Antigravity,
    CLIAgent::QoderCLI,
    CLIAgent::CodeBuddy,
];

#[derive(Clone, Copy)]
struct ProductIcon {
    label: L10nKey,
    standard_label: L10nKey,
    asset: Option<&'static str>,
}

static CRISP_RASTERS: LazyLock<Mutex<HashMap<(u8, u32, bool), Option<Arc<RenderImage>>>>> =
    LazyLock::new(Default::default);

fn srgb_to_linear(u: u8) -> f32 {
    let c = f32::from(u) / 255.;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0., 1.);
    let s = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    };
    (s * 255.).round().clamp(0., 255.) as u8
}

/// Downsamples an RGBA PNG in linear-light premultiplied alpha to the exact
/// target device pixel size so GPUI's un-mipmapped atlas does not alias edges
/// or bleed dark fringes from transparent pixels.
fn downsample_png(
    bytes: &[u8],
    crop: Option<(u32, u32, u32, u32)>,
    target_px: u32,
) -> Option<RenderImage> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let rgba = match crop {
        Some((x, y, w, h)) => decoded.crop_imm(x, y, w, h).into_rgba8(),
        None => decoded.into_rgba8(),
    };
    let (w, h) = rgba.dimensions();
    let mut premul = ImageBuffer::<Rgba<f32>, Vec<f32>>::new(w, h);
    for (dst, src) in premul.pixels_mut().zip(rgba.pixels()) {
        let a = f32::from(src[3]) / 255.;
        *dst = Rgba([
            srgb_to_linear(src[0]) * a,
            srgb_to_linear(src[1]) * a,
            srgb_to_linear(src[2]) * a,
            a,
        ]);
    }
    let resized = image::imageops::resize(&premul, target_px, target_px, FilterType::Lanczos3);
    let mut bgra = Vec::with_capacity((target_px * target_px * 4) as usize);
    for p in resized.pixels() {
        let a = p[3].clamp(0., 1.);
        if a <= 1e-4 {
            bgra.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let r = linear_to_srgb(p[0] / a);
            let g = linear_to_srgb(p[1] / a);
            let b = linear_to_srgb(p[2] / a);
            let a_u8 = (a * 255.).round().clamp(0., 255.) as u8;
            bgra.extend_from_slice(&[b, g, r, a_u8]);
        }
    }
    let buffer = RgbaImage::from_raw(target_px, target_px, bgra)?;
    Some(RenderImage::new(SmallVec::from_elem(Frame::new(buffer), 1)))
}

/// Rasterizes Qoder's 5x5 startup mark onto an exact integer multiple of 5
/// device pixels centered inside `target_px` so every block has equal pixel
/// width and razor-sharp edges at every display scale. Uses Qoder CLI's
/// `#27BD51` on dark surfaces and a deeper same-hue `#148436` on light
/// surfaces so contrast stays above WCAG UI minimums (>4.2:1).
fn render_qoder_startup(target_px: u32, dark: bool) -> Option<RenderImage> {
    let svg_bytes = include_bytes!("../../assets/icons/agents/qoder-startup.svg");
    let tree = usvg::Tree::from_data(svg_bytes, &usvg::Options::default()).ok()?;
    let cell = (target_px / 5).max(1);
    let grid = (cell * 5).min(target_px);
    let pad = ((target_px - grid) / 2) as f32;
    let mut pixmap = tiny_skia::Pixmap::new(target_px, target_px)?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let transform =
        tiny_skia::Transform::from_scale(grid as f32 / w, grid as f32 / h).post_translate(pad, pad);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Straight-alpha BGRA for GPUI RenderImage.
    let ink_bgra = if dark {
        [0x51, 0xBD, 0x27, 0xFF] // #27BD51
    } else {
        [0x36, 0x84, 0x14, 0xFF] // #148436 (same hue, >4.2:1 on light surfaces)
    };
    let mut bgra = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let a = p.alpha();
        if a == 0 {
            bgra.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            bgra.extend_from_slice(&[ink_bgra[0], ink_bgra[1], ink_bgra[2], a]);
        }
    }
    let buffer = RgbaImage::from_raw(target_px, target_px, bgra)?;
    Some(RenderImage::new(SmallVec::from_elem(Frame::new(buffer), 1)))
}

fn render_crisp_asset(agent: CLIAgent, target_px: u32, dark: bool) -> Option<RenderImage> {
    match agent {
        CLIAgent::Antigravity => {
            // The 540x540 brand PNG has generous transparent padding around a
            // 404x372 glyph (67..471, 84..456). Cropping symmetrically to
            // 460x460 matches the optical size of `antigravity.svg`.
            downsample_png(
                include_bytes!("../../assets/icons/agents/antigravity-full-color.png"),
                Some((39, 40, 460, 460)),
                target_px,
            )
        }
        CLIAgent::Codex => downsample_png(
            include_bytes!("../../assets/icons/agents/codex-product.png"),
            None,
            target_px,
        ),
        CLIAgent::QoderCLI | CLIAgent::QoderCLICn => render_qoder_startup(target_px, dark),
        _ => None,
    }
}

fn crisp_image_source(agent: CLIAgent, logical_size: f32, dark: bool) -> ImageSource {
    let key_agent = match agent {
        CLIAgent::QoderCLICn => CLIAgent::QoderCLI,
        other => other,
    };
    let theme_key = matches!(key_agent, CLIAgent::QoderCLI) && dark;
    ImageSource::Custom(Arc::new(move |window, _cx| {
        let target_px = (logical_size * window.scale_factor()).round().max(1.) as u32;
        CRISP_RASTERS
            .lock()
            .unwrap()
            .entry((key_agent as u8, target_px, theme_key))
            .or_insert_with(|| render_crisp_asset(key_agent, target_px, dark).map(Arc::new))
            .clone()
            .map(Ok)
    }))
}

fn product_icon(agent: CLIAgent) -> Option<ProductIcon> {
    let (label, standard_label, asset) = match agent {
        CLIAgent::Claude => (
            L10nKey::SettingsSidebarIconClawd,
            L10nKey::SettingsSidebarIconStarburst,
            None,
        ),
        CLIAgent::Codex => (
            L10nKey::SettingsSidebarIconApp,
            L10nKey::SettingsSidebarIconOpenAI,
            Some("icons/agents/codex-product.png"),
        ),
        CLIAgent::QoderCLI | CLIAgent::QoderCLICn => (
            L10nKey::SettingsSidebarIconPixelQ,
            L10nKey::SettingsSidebarIconOutline,
            Some("icons/agents/qoder-startup.svg"),
        ),
        _ => return None,
    };
    Some(ProductIcon {
        label,
        standard_label,
        asset,
    })
}

impl ProductIcon {
    fn image(self, claude: &Option<ImageSource>) -> ImageSource {
        match self.asset {
            Some(asset) => ImageSource::from(asset),
            None => claude
                .clone()
                .unwrap_or_else(|| ImageSource::from("icons/agents/claude-clawd.png")),
        }
    }
}

fn local_claude_icon() -> Option<ImageSource> {
    crate::core::config::config_path("agent-icons/claude.png")
        .filter(|path| path.is_file())
        .map(ImageSource::from)
}

pub(crate) fn selected_claude_icon(cx: &App) -> Option<ImageSource> {
    cx.global::<Config>()
        .sidebar_agent_custom_icons
        .contains(CLIAgent::Claude.slug())
        .then(local_claude_icon)
        .flatten()
}

pub(crate) fn selected_product_icon(
    agent: CLIAgent,
    claude: &Option<ImageSource>,
    config: &Config,
) -> Option<ImageSource> {
    if agent == CLIAgent::Antigravity {
        return Some(ImageSource::from("icons/agents/antigravity-full-color.png"));
    }
    if !config.sidebar_agent_custom_icons.contains(agent.slug()) {
        return None;
    }
    Some(product_icon(agent)?.image(claude))
}

/// Renders an agent's configured icon sized for a target standard-glyph extent
/// (such as the pane title row), keeping monochrome glyphs at `glyph_size` and
/// giving product artwork a matching centered box.
pub(crate) fn agent_icon_for_glyph(
    pane: gpui::EntityId,
    agent: CLIAgent,
    glyph_size: f32,
    cx: &App,
) -> AnyElement {
    let avatar_size = (glyph_size / STANDARD_GLYPH_RATIO).round();
    let claude = selected_claude_icon(cx);
    let artwork = selected_product_icon(agent, &claude, cx.global::<Config>());
    let kind = if artwork.is_some() {
        "custom"
    } else {
        "standard"
    };
    div()
        .debug_selector(move || format!("agent-pane-icon-{kind}-{}", pane.as_u64()))
        .flex_shrink_0()
        .size(px(avatar_size - 2.))
        .flex()
        .items_center()
        .justify_center()
        .child(agent_icon(agent, artwork, avatar_size, cx))
        .into_any_element()
}

pub(crate) fn agent_icon(
    agent: CLIAgent,
    artwork: Option<ImageSource>,
    size: f32,
    cx: &App,
) -> AnyElement {
    let ink = rgb(cx.global::<AgentIcons>().ink(agent));
    let fallback = move || {
        svg()
            .path(agent.icon_path())
            .size(px(size * STANDARD_GLYPH_RATIO))
            .text_color(ink)
            .into_any_element()
    };
    match artwork {
        Some(source) => {
            let dark = crate::ui::presets::surface_is_dark(cx.theme().sidebar);
            let image_size = if matches!(agent, CLIAgent::QoderCLI | CLIAgent::QoderCLICn) {
                (size * 0.68).round()
            } else {
                size - 2.
            };
            let source = match agent {
                CLIAgent::Antigravity
                | CLIAgent::Codex
                | CLIAgent::QoderCLI
                | CLIAgent::QoderCLICn => crisp_image_source(agent, image_size, dark),
                _ => source,
            };
            img(source)
                .size(px(image_size))
                .flex()
                .items_center()
                .justify_center()
                .with_fallback(fallback)
                .into_any_element()
        }
        None => fallback(),
    }
}

fn ordered_agents() -> Vec<CLIAgent> {
    use CLIAgent::{Aider, Amp, Auggie, Droid, Goose};

    let mut agents: Vec<_> = CLIAgent::ALL
        .into_iter()
        .filter(|agent| !matches!(agent, Aider | Amp | Auggie | Droid | Goose))
        .collect();
    agents.sort_by_key(|agent| {
        (
            FIRST
                .iter()
                .position(|first| first == agent)
                .unwrap_or(usize::MAX),
            agent.display_name().to_ascii_lowercase(),
        )
    });
    agents
}

fn agent_matches(agent: CLIAgent, query: &str) -> bool {
    agent.display_name().to_ascii_lowercase().contains(query)
        || agent.aliases().iter().any(|alias| alias.contains(query))
}

fn icon_label(agent: CLIAgent, artwork: Option<ImageSource>, label: &str, cx: &App) -> gpui::Div {
    h_flex()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(22.))
                .flex_shrink_0()
                .child(agent_icon(agent, artwork, 22., cx)),
        )
        .child(label.to_owned())
}

impl HittyApp {
    pub(crate) fn sidebar_agent_icon_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(settings) = self.active_settings() else {
            return div().into_any_element();
        };
        let open = settings.sidebar_icon_choices_open;
        let toggle = Button::new("sidebar-agent-icon-settings")
            .small()
            .label(t(if open {
                L10nKey::SettingsSidebarIconsCollapse
            } else {
                L10nKey::SettingsSidebarIconsCustomize
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(settings) = this.active_settings_mut() {
                    settings.sidebar_icon_choices_open = !settings.sidebar_icon_choices_open;
                    cx.notify();
                }
            }))
            .into_any_element();
        let mut section = v_flex().child(self.settings_row(
            t(L10nKey::SettingsSidebarAgentIcons),
            t(L10nKey::SettingsSidebarAgentIconsDesc),
            toggle,
            cx,
        ));
        if !open {
            return section.into_any_element();
        }

        let theme = cx.theme();
        let config = cx.global::<Config>();
        let claude = local_claude_icon();
        let query = settings
            .sidebar_icon_search
            .read(cx)
            .value()
            .trim()
            .to_ascii_lowercase();
        let matches: Vec<_> = ordered_agents()
            .into_iter()
            .filter(|agent| agent_matches(*agent, &query))
            .collect();
        let count = matches.len().to_string();
        let search = h_flex()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&settings.sidebar_icon_search).small()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t_fmt(
                        L10nKey::SettingsSidebarIconsCount,
                        &[("count", &count)],
                    )),
            );
        let list = v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(search)
            .when(matches.is_empty(), |list| {
                list.child(
                    div()
                        .text_sm()
                        .child(t(L10nKey::SettingsSidebarIconsNoMatches)),
                )
            })
            .children(
                matches
                    .into_iter()
                    .map(|agent| self.sidebar_agent_icon_row(agent, &claude, cx)),
            );

        let preview = v_flex()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .bg(theme.sidebar)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t(L10nKey::SettingsSidebarIconsPreview)),
            )
            .children(FIRST.into_iter().map(|agent| {
                icon_label(
                    agent,
                    selected_product_icon(agent, &claude, config),
                    agent.display_name(),
                    cx,
                )
            }));

        let content = if self.settings_row_under(620., cx) {
            v_flex().gap_3().child(preview).child(list)
        } else {
            h_flex()
                .items_start()
                .gap_4()
                .child(list)
                .child(preview.w(px(175.)).flex_shrink_0())
        };
        section = section.child(content.pt_2().pb_4());
        section.into_any_element()
    }

    fn sidebar_agent_icon_row(
        &self,
        agent: CLIAgent,
        claude: &Option<ImageSource>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let theme = cx.theme();
        let config = cx.global::<Config>();
        let choice = product_icon(agent);
        let custom = config.sidebar_agent_custom_icons.contains(agent.slug());
        let control = match choice {
            Some(choice) => {
                let custom_image = Some(choice.image(claude));
                let control = Button::new(("sidebar-agent-icon-choice", agent as usize))
                    .small()
                    .child(icon_label(
                        agent,
                        if custom { custom_image.clone() } else { None },
                        t(if custom {
                            choice.label
                        } else {
                            choice.standard_label
                        }),
                        cx,
                    ))
                    .child(Icon::new(IconName::ChevronDown).xsmall());
                let app = cx.entity().downgrade();
                control
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let default_label = t_fmt(
                            L10nKey::SettingsSidebarIconDefaultChoice,
                            &[("icon", t(choice.standard_label))],
                        );
                        [
                            (false, default_label, None),
                            (true, t(choice.label).to_owned(), custom_image.clone()),
                        ]
                        .into_iter()
                        .fold(menu, |menu, (use_custom, label, image)| {
                            let app = app.clone();
                            menu.item(
                                PopupMenuItem::element(move |_, cx| {
                                    icon_label(agent, image.clone(), &label, cx)
                                })
                                .checked(custom == use_custom)
                                .on_click(move |_, _, cx| {
                                    if let Some(app) = app.upgrade() {
                                        app.update(cx, |this, cx| {
                                            this.update_config(cx, |cfg| {
                                                let icons = &mut cfg.sidebar_agent_custom_icons;
                                                if use_custom {
                                                    icons.insert(agent.slug().into());
                                                } else {
                                                    icons.remove(agent.slug());
                                                }
                                            });
                                        });
                                    }
                                }),
                            )
                        })
                    })
                    .into_any_element()
            }
            None => icon_label(
                agent,
                selected_product_icon(agent, claude, config),
                t(L10nKey::SettingsSidebarIconDefault),
                cx,
            )
            .text_color(theme.muted_foreground)
            .into_any_element(),
        };
        h_flex()
            .justify_between()
            .gap_2()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().child(agent.display_name()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(agent.aliases()[0]),
                    ),
            )
            .child(div().flex_shrink_0().child(control))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_lists_visible_agents_in_preferred_order_and_searches_commands() {
        let agents = ordered_agents();
        assert_eq!(&agents[..FIRST.len()], &FIRST);
        let hidden = ["aider", "amp", "auggie", "droid", "goose"];
        let listed: std::collections::BTreeSet<_> =
            agents.iter().map(|agent| agent.slug()).collect();
        for agent in CLIAgent::ALL {
            assert_eq!(
                listed.contains(agent.slug()),
                !hidden.contains(&agent.slug())
            );
        }
        assert!(
            agents[FIRST.len()..]
                .windows(2)
                .all(|pair| pair[0].display_name().to_ascii_lowercase()
                    <= pair[1].display_name().to_ascii_lowercase())
        );
        for (query, expected) in [("agy", CLIAgent::Antigravity), ("cbc", CLIAgent::CodeBuddy)] {
            assert_eq!(
                agents
                    .iter()
                    .copied()
                    .filter(|agent| agent_matches(*agent, query))
                    .collect::<Vec<_>>(),
                [expected]
            );
        }
    }

    #[test]
    fn default_sidebar_icons_use_standard_marks_and_opt_in_custom_artwork() {
        let mut config = Config::default();
        assert!(selected_product_icon(CLIAgent::Claude, &None, &config).is_none());
        assert!(selected_product_icon(CLIAgent::Codex, &None, &config).is_none());
        assert!(selected_product_icon(CLIAgent::QoderCLI, &None, &config).is_none());
        assert!(selected_product_icon(CLIAgent::QoderCLICn, &None, &config).is_none());
        assert!(selected_product_icon(CLIAgent::Antigravity, &None, &config).is_some());

        for agent in [
            CLIAgent::Claude,
            CLIAgent::Codex,
            CLIAgent::QoderCLI,
            CLIAgent::QoderCLICn,
        ] {
            config
                .sidebar_agent_custom_icons
                .insert(agent.slug().into());
            assert!(
                selected_product_icon(agent, &None, &config).is_some(),
                "{} should resolve to custom artwork when opted in",
                agent.display_name()
            );
        }
    }

    #[test]
    fn crisp_rasters_match_device_pixels_and_qoder_stays_sharp_in_both_themes() {
        for size in [15, 20, 26, 30, 35, 40] {
            for agent in [CLIAgent::Antigravity, CLIAgent::Codex] {
                let image = render_crisp_asset(agent, size, true)
                    .unwrap_or_else(|| panic!("{} raster at {size}px", agent.display_name()));
                let bytes = image.as_bytes(0).unwrap();
                assert_eq!(bytes.len(), (size * size * 4) as usize);
                assert!(
                    bytes.chunks_exact(4).any(|p| p[3] == 255),
                    "{} raster at {size}px should contain opaque pixels",
                    agent.display_name()
                );
            }

            let cell = (size / 5).max(1);
            for (dark, expected) in [
                (true, [0x51, 0xBD, 0x27, 0xFF]),
                (false, [0x36, 0x84, 0x14, 0xFF]),
            ] {
                for agent in [CLIAgent::QoderCLI, CLIAgent::QoderCLICn] {
                    let qoder = render_crisp_asset(agent, size, dark).expect("qoder raster");
                    let bytes = qoder.as_bytes(0).unwrap();
                    assert_eq!(bytes.len(), (size * size * 4) as usize);
                    for px in bytes.chunks_exact(4) {
                        assert!(
                            px == [0, 0, 0, 0] || px == expected,
                            "qoder pixel at {size}px (dark={dark}) should stay crisp: {px:?}"
                        );
                    }
                    let filled = bytes.chunks_exact(4).filter(|px| **px == expected).count();
                    // The 5x5 startup glyph fills 13 unit cells (3+1+1+1+1+1+1+1+2+1).
                    assert_eq!(
                        filled,
                        13 * (cell * cell) as usize,
                        "qoder 13-cell glyph at {size}px should have uniform {cell}x{cell} blocks"
                    );
                }
            }
        }
    }
}

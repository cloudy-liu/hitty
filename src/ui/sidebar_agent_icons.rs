use gpui::{Anchor, AnyElement, App, Context, ImageSource, div, img, prelude::*, px, rgb, svg};
use gpui_component::button::Button;
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};

use crate::core::{cli_agent::CLIAgent, config::Config};
use crate::ui::app::HittyApp;
use crate::ui::i18n::{L10nKey, t, t_fmt};
use crate::ui::presets::AgentIcons;

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
        CLIAgent::QoderCLI => (
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
    fn image(self, claude: &Option<ImageSource>) -> Option<ImageSource> {
        self.asset.map(ImageSource::from).or_else(|| claude.clone())
    }
}

pub(crate) fn local_claude_icon() -> Option<ImageSource> {
    crate::core::config::config_path("agent-icons/claude.png")
        .filter(|path| path.is_file())
        .map(ImageSource::from)
}

pub(crate) fn selected_product_icon(
    agent: CLIAgent,
    claude: &Option<ImageSource>,
    config: &Config,
) -> Option<ImageSource> {
    if config.sidebar_agent_standard_icons.contains(agent.slug()) {
        return None;
    }
    product_icon(agent)?.image(claude)
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
            .size(px(size * 0.78))
            .text_color(ink)
            .into_any_element()
    };
    match artwork {
        Some(source) => {
            let image_size = if agent == CLIAgent::QoderCLI {
                size * 0.68
            } else {
                size - 2.
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

fn ordered_agents() -> [CLIAgent; CLIAgent::ALL.len()] {
    let mut agents = CLIAgent::ALL;
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
        let choice = product_icon(agent);
        let standard = cx
            .global::<Config>()
            .sidebar_agent_standard_icons
            .contains(agent.slug());
        let control = match choice {
            Some(choice) => {
                let default_image = choice.image(claude);
                let control = Button::new(("sidebar-agent-icon-choice", agent as usize))
                    .small()
                    .child(icon_label(
                        agent,
                        if standard {
                            None
                        } else {
                            default_image.clone()
                        },
                        t(if standard {
                            choice.standard_label
                        } else {
                            choice.label
                        }),
                        cx,
                    ))
                    .child(Icon::new(IconName::ChevronDown).xsmall());
                let app = cx.entity().downgrade();
                control
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let default_label = t_fmt(
                            L10nKey::SettingsSidebarIconDefaultChoice,
                            &[("icon", t(choice.label))],
                        );
                        [
                            (false, default_label, default_image.clone()),
                            (true, t(choice.standard_label).to_owned(), None),
                        ]
                        .into_iter()
                        .fold(
                            menu,
                            |menu, (use_standard, label, image)| {
                                let app = app.clone();
                                menu.item(
                                    PopupMenuItem::element(move |_, cx| {
                                        icon_label(agent, image.clone(), &label, cx)
                                    })
                                    .checked(standard == use_standard)
                                    .on_click(
                                        move |_, _, cx| {
                                            if let Some(app) = app.upgrade() {
                                                app.update(cx, |this, cx| {
                                                    this.update_config(cx, |cfg| {
                                                        let icons =
                                                            &mut cfg.sidebar_agent_standard_icons;
                                                        if use_standard {
                                                            icons.insert(agent.slug().into());
                                                        } else {
                                                            icons.remove(agent.slug());
                                                        }
                                                    });
                                                });
                                            }
                                        },
                                    ),
                                )
                            },
                        )
                    })
                    .into_any_element()
            }
            None => icon_label(agent, None, t(L10nKey::SettingsSidebarIconDefault), cx)
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
    fn sidebar_lists_all_agents_in_preferred_order_and_searches_commands() {
        let agents = ordered_agents();
        assert_eq!(&agents[..FIRST.len()], &FIRST);
        assert_eq!(
            agents
                .into_iter()
                .map(CLIAgent::slug)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            CLIAgent::ALL.len()
        );
        assert!(
            agents[FIRST.len()..]
                .windows(2)
                .all(|pair| pair[0].display_name().to_ascii_lowercase()
                    <= pair[1].display_name().to_ascii_lowercase())
        );
        for (query, expected) in [("agy", CLIAgent::Antigravity), ("cbc", CLIAgent::CodeBuddy)] {
            assert_eq!(
                agents
                    .into_iter()
                    .filter(|agent| agent_matches(*agent, query))
                    .collect::<Vec<_>>(),
                [expected]
            );
        }
    }
}

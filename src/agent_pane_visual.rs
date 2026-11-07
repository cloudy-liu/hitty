//! Opt-in screenshots of agent panes in a split: the identity row and the
//! hover toolbar beside an ordinary terminal. Deterministic daemon messages
//! keep the base/head comparison identical; the title setting travels as JSON
//! so the same fixture builds against a baseline that has no such setting.

use gpui::{prelude::*, *};
use gpui_component::Root;
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

use crate::{
    core::{
        actions::ToggleMaximizePane,
        cli_agent::{AgentSessionState, AgentStatus, CLIAgent},
        config::{Config, TabBarPosition},
    },
    daemon::{protocol::DaemonMsg, transport::Stream},
    terminal::view::{TerminalView, quiet_test_pane},
    ui::{
        app::{Tab, Tty7App},
        pane::{Pane, PaneSlot},
    },
};

#[derive(Deserialize)]
struct Options {
    output: PathBuf,
    #[serde(default)]
    titles: bool,
    #[serde(default)]
    dark: bool,
    #[serde(default)]
    zoom: bool,
    /// Open Settings on Window & Tabs instead of showing the panes.
    #[serde(default)]
    settings: bool,
    /// "grid" (2x2, default) or "column": one wide pane beside three stacked.
    #[serde(default)]
    layout: String,
    height: Option<f32>,
    hold_seconds: Option<u64>,
}

struct Fixture {
    owner: Entity<Tty7App>,
    _streams: Vec<Stream>,
}

impl Render for Fixture {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.owner.clone()
    }
}

const CASES: [(&str, Option<CLIAgent>, &str); 4] = [
    (
        "Claude Code",
        Some(CLIAgent::Claude),
        "> Add a title row to every agent pane\r\n\r\n  Reading src/ui/pane.rs\r\n",
    ),
    (
        "Codex",
        Some(CLIAgent::Codex),
        "> Check the title row's spacing\r\n\r\n  Waiting for the visual direction\r\n",
    ),
    (
        "Cursor",
        Some(CLIAgent::Cursor),
        "> Check narrow panes\r\n\r\n  Icon and name truncate within the row\r\n",
    ),
    (
        "PowerShell",
        None,
        "PS D:\\gh-prj\\ctty7> git status\r\nOn branch main\r\nnothing to commit, working tree clean\r\nPS D:\\gh-prj\\ctty7> ",
    ),
];

pub(crate) fn run(path: &std::ffi::OsStr) {
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    gpui_platform::application()
        .with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            gpui_component::init(cx);
            crate::register_bundled_fonts(cx);
            let mut config = Config::default();
            config.theme_follow_system = false;
            config.theme_preset = if options.dark { "dark" } else { "light" }.into();
            config.install_cli_on_path = false;
            config.cursor_blink = false;
            config.sidebar_width = 240.;
            config.tab_bar_position = TabBarPosition::Left;
            // Named as JSON, not as a field: the baseline this fixture is also
            // built against does not know the setting and ignores the key.
            let mut value = serde_json::to_value(&config.0).unwrap();
            value["show_agent_pane_titles"] = serde_json::Value::Bool(options.titles);
            let config = Config(serde_json::from_value(value).unwrap());
            cx.set_global(config);
            crate::core::session::WorkspaceStore::init(cx);
            crate::ui::windows::WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            let mut views: Vec<Entity<TerminalView>> = Vec::new();
            let mut owner_entity = None;
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(1200.), px(options.height.unwrap_or(700.))),
                            cx,
                        ))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Agent pane visual acceptance".into()),
                            ..gpui_component::TitleBar::title_bar_options()
                        }),
                        show: true,
                        focus: true,
                        ..Default::default()
                    },
                    |window, cx| {
                        crate::ui::theme::apply_theme(Some(window), cx);
                        let owner = cx.new(|cx| {
                            Tty7App::with_session(
                                None,
                                Some(crate::core::session::Session::default()),
                                window,
                                cx,
                            )
                        });
                        let mut streams = Vec::new();
                        owner.update(cx, |app, cx| {
                            for (index, (name, agent, screen)) in CASES.iter().enumerate() {
                                let (view, mut stream) =
                                    quiet_test_pane(index as u64 + 1, window, cx);
                                DaemonMsg::Agent(*agent).encode(&mut stream).unwrap();
                                if agent.is_some() {
                                    DaemonMsg::AgentStatus(Some(AgentSessionState {
                                        status: AgentStatus::Working,
                                        rich: true,
                                        ..Default::default()
                                    }))
                                    .encode(&mut stream)
                                    .unwrap();
                                }
                                DaemonMsg::Output(
                                    format!("\x1b]0;{name}\x07{screen}").into_bytes(),
                                )
                                .encode(&mut stream)
                                .unwrap();
                                views.push(view);
                                streams.push(stream);
                            }
                            let leaf =
                                |v: &Entity<TerminalView>| Pane::leaf(PaneSlot::Ready(v.clone()));
                            let tree = if options.layout == "column" {
                                // One wide pane beside a column of three: the
                                // shape in which review found agent names cut
                                // down to an ellipsis.
                                let lower = Pane::split_node(
                                    Axis::Vertical,
                                    0.5,
                                    leaf(&views[2]),
                                    leaf(&views[3]),
                                );
                                let column =
                                    Pane::split_node(Axis::Vertical, 0.4, leaf(&views[1]), lower);
                                Pane::split_node(Axis::Horizontal, 0.5, leaf(&views[0]), column)
                            } else {
                                let top = Pane::split_node(
                                    Axis::Horizontal,
                                    0.5,
                                    leaf(&views[0]),
                                    leaf(&views[1]),
                                );
                                let bottom = Pane::split_node(
                                    Axis::Horizontal,
                                    0.5,
                                    leaf(&views[2]),
                                    leaf(&views[3]),
                                );
                                Pane::split_node(Axis::Vertical, 0.5, top, bottom)
                            };
                            app.tabs.push(Tab::new(tree));
                            let top = Pane::split_node(
                                Axis::Horizontal,
                                0.5,
                                leaf(&views[0]),
                                leaf(&views[1]),
                            );
                            let bottom = Pane::split_node(
                                Axis::Horizontal,
                                0.5,
                                leaf(&views[2]),
                                leaf(&views[3]),
                            );
                            app.tabs.push(Tab::new(Pane::split_node(
                                Axis::Vertical,
                                0.5,
                                top,
                                bottom,
                            )));
                            app.active = 0;
                            app.focus_active(window, cx);
                            cx.notify();
                        });
                        owner_entity = Some(owner.clone());
                        let fixture = cx.new(|_| Fixture {
                            owner,
                            _streams: streams,
                        });
                        cx.new(|cx| Root::new(fixture, window, cx))
                    },
                )
                .unwrap();
            cx.activate(true);
            let owner = owner_entity.unwrap();
            cx.spawn(async move |cx| {
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    let ready = views
                        .iter()
                        .zip(CASES.iter())
                        .all(|(view, (name, agent, _))| {
                            view.read_with(cx, |view, _| {
                                view.title == *name && view.agent() == *agent
                            })
                        });
                    if ready {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "agent pane fixture did not settle"
                    );
                }
                cx.update_window(handle.into(), |_, window, cx| {
                    owner.update(cx, |app, cx| {
                        app.focus_active(window, cx);
                        if options.settings {
                            app.open_settings_section(
                                crate::ui::settings::SettingsSection::WindowTabs,
                                window,
                                cx,
                            );
                        }
                    });
                    if options.zoom {
                        window.dispatch_action(Box::new(ToggleMaximizePane), cx);
                    }
                    window.refresh();
                    let _ = window.draw(cx);
                })
                .unwrap();
                cx.background_executor().timer(Duration::from_secs(2)).await;
                cx.update_window(handle.into(), |_, window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                    window.set_window_title("Agent pane visual acceptance");
                })
                .unwrap();
                eprintln!("NATIVE_AGENT_PANES_READY");
                let done = options.output.with_extension("done");
                let deadline = std::time::Instant::now()
                    + Duration::from_secs(options.hold_seconds.unwrap_or(60));
                while !done.exists() && std::time::Instant::now() < deadline {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                }
                // The platform input handler owns the focused terminal. Release
                // it while the window can still draw, before GPUI checks leaks.
                cx.update_window(handle.into(), |_, window, cx| {
                    window.blur();
                    window.refresh();
                    let _ = window.draw(cx);
                    window.remove_window();
                })
                .unwrap();
                drop(views);
                drop(owner);
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}

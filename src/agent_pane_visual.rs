//! Opt-in screenshots of agent panes in a split: the identity row and the
//! hover toolbar beside an ordinary terminal. Deterministic daemon messages
//! keep the base/head comparison identical; the title setting travels as JSON
//! so the same fixture builds against a baseline that has no such setting.

use gpui::{prelude::*, *};
use gpui_component::Root;
use hitty_core::{core::machine::PaneRecord, host::HostId};
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
        app::{HittyApp, Tab},
        pane::{Pane, PaneSlot},
    },
};

#[derive(Deserialize)]
struct Options {
    output: PathBuf,
    #[serde(default)]
    titles: bool,
    /// Agent panes title themselves with a task, as a live agent does, instead
    /// of with their own name.
    #[serde(default)]
    tasks: bool,
    /// `show_agent_pane_task_titles`.
    #[serde(default)]
    task_titles: bool,
    #[serde(default)]
    sidebar_product_icons: bool,
    /// Include Qoder and Antigravity in both revisions for icon comparison.
    #[serde(default)]
    sidebar_icon_fixture: bool,
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
    activity_ages: Option<[u64; 3]>,
    activity_reference_secs: Option<u64>,
    #[serde(default)]
    short_title: bool,
    sidebar_width: Option<f32>,
    ui_font_size: Option<f32>,
    height: Option<f32>,
    hold_seconds: Option<u64>,
    #[serde(default)]
    pinned_dirs: Vec<PathBuf>,
    #[serde(default)]
    settings_section: Option<String>,
    #[serde(default)]
    agent_launch_commands: std::collections::BTreeMap<String, String>,
}

struct Fixture {
    owner: Entity<HittyApp>,
    _streams: Vec<Stream>,
}

impl Render for Fixture {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.owner.clone()
    }
}

/// Name, agent, the title that agent would write for its task, and its screen.
const CASES: [(&str, Option<CLIAgent>, &str, &str); 4] = [
    (
        "Claude Code",
        Some(CLIAgent::Claude),
        "\u{25D0} Hitty \u{5206}\u{5C4F}\u{7EC8}\u{7AEF}\u{542F}\u{52A8} 10 \u{500D}\u{63D0}\u{901F}",
        "> Add a title row to every agent pane\r\n\r\n  Reading src/ui/pane.rs\r\n",
    ),
    (
        "Codex",
        Some(CLIAgent::Codex),
        "\u{2807} \u{67E5}\u{770B}\u{4F1A}\u{8BDD}\u{8BA8}\u{8BBA}\u{5185}\u{5BB9} | hitty",
        "> Check the title row's spacing\r\n\r\n  Waiting for the visual direction\r\n",
    ),
    (
        "Cursor",
        Some(CLIAgent::Cursor),
        "Check narrow panes and cut a long task title before it reaches the toolbar",
        "> Check narrow panes\r\n\r\n  Icon and name truncate within the row\r\n",
    ),
    (
        "PowerShell",
        None,
        "PowerShell",
        "PS D:\\gh-prj\\hitty> git status\r\nOn branch main\r\nnothing to commit, working tree clean\r\nPS D:\\gh-prj\\hitty> ",
    ),
];

const ICON_CASES: [(&str, Option<CLIAgent>, &str, &str); 4] = [
    CASES[0],
    CASES[1],
    (
        "Qoder",
        Some(CLIAgent::QoderCLI),
        "Update terminal docs",
        "Qoder CLI\r\n",
    ),
    (
        "Antigravity",
        Some(CLIAgent::Antigravity),
        "Check startup",
        "Antigravity CLI\r\n",
    ),
];

/// What a pane's title settles on: its task when the scene asks for tasks and
/// it is an agent's, else its name.
fn pane_title(
    case: &(&'static str, Option<CLIAgent>, &'static str, &str),
    tasks: bool,
) -> &'static str {
    match case.1 {
        Some(_) if tasks => case.2,
        _ => case.0,
    }
}

pub(crate) fn run(path: &std::ffi::OsStr) {
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    gpui_platform::application()
        .with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            gpui_component::init(cx);
            crate::register_bundled_fonts(cx);
            let mut config = Config::default();
            let sidebar_activity = options.layout == "sidebar-activity";
            let pinned_empty = options.layout == "pinned-empty";
            let pinned_grouped = options.layout == "pinned-grouped";
            let cases = if options.sidebar_icon_fixture {
                &ICON_CASES
            } else {
                &CASES
            };
            config.theme_follow_system = false;
            config.theme_preset = if options.dark { "dark" } else { "light" }.into();
            config.install_cli_on_path = false;
            config.cursor_blink = false;
            config.sidebar_width =
                options
                    .sidebar_width
                    .unwrap_or(if sidebar_activity { 206. } else { 240. });
            if let Some(size) = options.ui_font_size {
                config.ui_font_size = size;
            }
            if sidebar_activity {
                config.sidebar_grouping = crate::core::config::SidebarGrouping::None;
            } else if pinned_empty || pinned_grouped {
                config.sidebar_grouping = crate::core::config::SidebarGrouping::RepoOrDirectory;
                config.sidebar_pinned_groups = options
                    .pinned_dirs
                    .iter()
                    .map(|dir| dir.to_string_lossy().into_owned())
                    .collect();
            }
            config.tab_bar_position = TabBarPosition::Left;
            // Named as JSON, not as a field: the baseline this fixture is also
            // built against does not know the setting and ignores the key.
            let mut value = serde_json::to_value(&config.0).unwrap();
            value["show_agent_pane_titles"] = serde_json::Value::Bool(options.titles);
            value["show_agent_pane_task_titles"] = serde_json::Value::Bool(options.task_titles);
            if !options.agent_launch_commands.is_empty() {
                value["agent_launch_commands"] =
                    serde_json::to_value(&options.agent_launch_commands).unwrap();
            }
            let names_key = if value.get("sidebar_agent_names").is_some() {
                "sidebar_agent_names"
            } else {
                "sidebar_agent_product_icons"
            };
            value[names_key] = serde_json::Value::Bool(options.sidebar_product_icons);
            if options.sidebar_icon_fixture {
                let saved = serde_json::to_value(&Config::load().0).unwrap();
                if let Some(icons) = saved.get("sidebar_agent_custom_icons") {
                    value["sidebar_agent_custom_icons"] = icons.clone();
                }
            }
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
                        let workspace = crate::core::session::WorkspaceStore::claim(cx, None);
                        let owner = cx.new(|cx| {
                            HittyApp::with_session(
                                Some(workspace),
                                Some(crate::core::session::Session::default()),
                                window,
                                cx,
                            )
                        });
                        let mut streams = Vec::new();
                        owner.update(cx, |app, cx| {
                            for dir in &options.pinned_dirs {
                                crate::core::session::WorkspaceStore::remember_sidebar_folder(cx, workspace, dir.clone());
                                app.sidebar_dir_cache.borrow_mut().insert(dir.clone(), true);
                            }
                            if pinned_empty {
                                cx.notify();
                                return;
                            }
                            if pinned_grouped {
                                let cwd = options
                                    .pinned_dirs
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| options.output.parent().unwrap().to_path_buf());
                                let (view, mut stream) = quiet_test_pane(1, window, cx);
                                DaemonMsg::Cwd(cwd.clone()).encode(&mut stream).unwrap();
                                DaemonMsg::Output(
                                    format!(
                                        "\x1b]0;cmd.exe\x07Microsoft Windows [Version 10.0]\r\n\r\n{}> ",
                                        cwd.display()
                                    )
                                    .into_bytes(),
                                )
                                .encode(&mut stream)
                                .unwrap();
                                let tab = Tab::new(Pane::leaf(PaneSlot::Ready(view.clone())));
                                *tab.sidebar_group.borrow_mut() = Some(cwd);
                                app.tabs.push(tab);
                                app.active = 0;
                                app.focus_active(window, cx);
                                views.push(view);
                                streams.push(stream);
                                cx.notify();
                                return;
                            }
                            for (index, case) in cases.iter().enumerate() {
                                let (_, agent, _, screen) = case;
                                let name = if options.short_title && index == 0 {
                                    "hitty"
                                } else {
                                    pane_title(case, options.tasks)
                                };
                                let (view, mut stream) =
                                    quiet_test_pane(index as u64 + 1, window, cx);
                                if sidebar_activity {
                                    seed_sidebar_repo(index, &options.output, &mut stream, cx);
                                }
                                DaemonMsg::Agent(*agent).encode(&mut stream).unwrap();
                                if agent.is_some() {
                                    DaemonMsg::AgentStatus(Some(AgentSessionState {
                                        status: if options.sidebar_icon_fixture {
                                            [
                                                AgentStatus::Waiting,
                                                AgentStatus::Working,
                                                AgentStatus::Idle,
                                                AgentStatus::Done,
                                            ][index]
                                        } else if options.sidebar_product_icons {
                                            [
                                                AgentStatus::Waiting,
                                                AgentStatus::Idle,
                                                AgentStatus::Done,
                                                AgentStatus::Idle,
                                            ][index]
                                        } else {
                                            AgentStatus::Working
                                        },
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
                            if sidebar_activity {
                                app.tabs
                                    .extend(views.iter().map(|view| Tab::new(leaf(view))));
                                app.focus_active(window, cx);
                                cx.notify();
                                return;
                            }
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
                    let ready = if pinned_empty {
                        true
                    } else if pinned_grouped {
                        views
                            .first()
                            .is_some_and(|view| view.read_with(cx, |view, _| view.title == "cmd.exe"))
                    } else {
                        views
                            .iter()
                            .zip(cases.iter())
                            .enumerate()
                            .all(|(index, (view, case))| {
                                let title = if options.short_title && index == 0 {
                                    "hitty"
                                } else {
                                    pane_title(case, options.tasks)
                                };
                                view.read_with(cx, |view, _| {
                                    view.title == title && view.agent() == case.1
                                })
                            })
                    };
                    if ready {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "agent pane fixture did not settle"
                    );
                }
                if options.settings_section.as_deref() == Some("agents") {
                    // Let off-thread agent PATH discovery finish before opening
                    // Settings so quick-launch placeholders match detected CLIs.
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                }
                cx.update_window(handle.into(), |_, window, cx| {
                    owner.update(cx, |app, cx| {
                        app.focus_active(window, cx);
                        if options.settings {
                            let section = match options.settings_section.as_deref() {
                                Some("agents") => crate::ui::settings::SettingsSection::Agents,
                                _ => crate::ui::settings::SettingsSection::WindowTabs,
                            };
                            app.open_settings_section(section, window, cx);
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
                    let row_heights = |cx: &App| {
                        owner
                            .read(cx)
                            .sidebar_slots
                            .borrow()
                            .iter()
                            .map(|bounds| bounds.size.height.as_f32())
                            .collect::<Vec<_>>()
                    };
                    let without_activity = row_heights(cx);
                    if sidebar_activity {
                        seed_sidebar_activity(
                            options.activity_ages.unwrap_or([180, 3600, 172_800]),
                            options.activity_reference_secs,
                            cx,
                        );
                    }
                    window.refresh();
                    let _ = window.draw(cx);
                    if sidebar_activity {
                        std::fs::write(
                            options.output.with_extension("layout.json"),
                            serde_json::to_vec_pretty(&serde_json::json!({
                                "without_activity": without_activity,
                                "with_activity": row_heights(cx),
                                "display_scale": window.scale_factor(),
                            }))
                            .unwrap(),
                        )
                        .unwrap();
                    }
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

fn seed_sidebar_repo(index: usize, output: &std::path::Path, stream: &mut Stream, cx: &mut App) {
    use crate::terminal::git_status::{GitStatusCache, RepoSnapshot};

    let cwd = output.with_file_name(format!("fixture-repo-{index}"));
    DaemonMsg::Cwd(cwd.clone()).encode(stream).unwrap();
    cx.default_global::<GitStatusCache>();
    cx.update_global::<GitStatusCache, _>(|cache, _| {
        let (branch, counts) = if index == 0 {
            (
                "feature/sidebar-activity-and-recover-after-restart",
                (18, 4),
            )
        } else {
            ("main", (0, 0))
        };
        cache.finish_probe(
            HostId::LOCAL,
            &cwd,
            Some(RepoSnapshot {
                root: cwd.clone(),
                home: cwd.clone(),
                branch: branch.into(),
                counts: Some(counts),
            }),
        );
    });
}

fn seed_sidebar_activity(ages: [u64; 3], reference_secs: Option<u64>, cx: &mut App) {
    let mut machine = hitty_core::core::machine::Machine::default();
    let now = reference_secs.unwrap_or_else(crate::ui::home::now_secs);
    for (index, age) in ages.into_iter().enumerate() {
        let pane = PaneRecord::new(index as u64 + 1);
        let mut value = serde_json::to_value(pane).unwrap();
        // The baseline ignores this field, while the fixture renders the
        // same workspace and pane facts through the production sidebar.
        value["last_agent_activity"] = serde_json::json!({
            "unix_ms": (now - age) * 1000,
            "kind": "turn_finished",
        });
        machine.panes.push(serde_json::from_value(value).unwrap());
    }
    crate::ui::machine_mirror::MachineMirrors::install(cx, HostId::LOCAL, machine);
}

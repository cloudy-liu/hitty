//! Opt-in screenshots of the production tab strip and sidebar.
//! Deterministic daemon messages keep the base/head comparison identical.

use gpui::{prelude::*, *};
use gpui_component::Root;
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

use crate::{
    core::{
        cli_agent::{AgentSessionState, AgentStatus, CLIAgent},
        config::{Config, SidebarGrouping, TabBarPosition},
    },
    daemon::{
        protocol::{DaemonMsg, RemoteContext, RemoteKind},
        transport::Stream,
    },
    terminal::view::{TerminalView, quiet_test_pane},
    ui::{
        app::{Tab, Tty7App},
        pane::{Pane, PaneSlot},
    },
};

#[derive(Deserialize)]
struct Options {
    sidebar: bool,
    output: PathBuf,
    hold_seconds: Option<u64>,
    #[serde(default)]
    agent_replay: bool,
    #[serde(default)]
    claude_question: Option<PathBuf>,
    #[serde(default)]
    auto_names: bool,
    #[serde(default)]
    attention_badge: bool,
    sidebar_group_arrows: Option<bool>,
    sidebar_group_folders: Option<bool>,
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

const CASES: [(&str, Option<CLIAgent>, Option<RemoteKind>); 6] = [
    ("Local terminal", None, None),
    ("Local Codex", Some(CLIAgent::Codex), None),
    ("Local Claude", Some(CLIAgent::Claude), None),
    ("prod-web", None, Some(RemoteKind::NativeSsh)),
    ("Fix login", Some(CLIAgent::Codex), Some(RemoteKind::Ssh)),
    (
        "Deploy check",
        Some(CLIAgent::Claude),
        Some(RemoteKind::NativeSsh),
    ),
];

pub(crate) fn run(path: &std::ffi::OsStr) {
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    gpui_platform::application()
        .with_assets(crate::ui::assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            let screen_replay = options.agent_replay || options.claude_question.is_some();
            gpui_component::init(cx);
            crate::register_bundled_fonts(cx);
            let mut config = Config::default();
            config.theme_follow_system = false;
            config.theme_preset = if screen_replay { "dark" } else { "light" }.into();
            config.install_cli_on_path = false;
            config.cursor_blink = false;
            config.sidebar_width = 260.;
            if let Some(arrows) = options.sidebar_group_arrows {
                config.sidebar_group_arrows = arrows;
            }
            if let Some(folders) = options.sidebar_group_folders {
                config.sidebar_group_folders = folders;
            }
            // The sidebar captures exist to show grouping behaviour: local
            // sessions land in Scratch, in-pane SSH hops split out per host.
            config.sidebar_grouping = if options.sidebar {
                SidebarGrouping::Repo
            } else {
                SidebarGrouping::None
            };
            config.tab_bar_position = if options.sidebar {
                TabBarPosition::Left
            } else {
                TabBarPosition::Top
            };
            cx.set_global(config);
            crate::core::session::WorkspaceStore::init(cx);
            crate::ui::windows::WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            let mut views: Vec<Entity<TerminalView>> = Vec::new();
            let mut owner_entity = None;
            let mut replay_stream = None;
            let cases = if options.attention_badge {
                vec![
                    ("Completed Agent", Some(CLIAgent::Claude), None),
                    ("Other terminal", None, None),
                ]
            } else if options.claude_question.is_some() {
                vec![("Claude question replay", Some(CLIAgent::Claude), None)]
            } else if options.agent_replay {
                vec![("AGY screen replay", Some(CLIAgent::Antigravity), None)]
            } else if options.auto_names {
                vec![
                    ("Local terminal", None, None),
                    ("deploy@prod-web:~", None, Some(RemoteKind::NativeSsh)),
                    ("deploy@prod-web:~/projects/api", None, Some(RemoteKind::NativeSsh)),
                    ("dev@dev-box:~/projects/api", None, Some(RemoteKind::Ssh)),
                    ("Fix login", Some(CLIAgent::Codex), Some(RemoteKind::Ssh)),
                    ("deploy@prod-web:/var/log/nginx", None, Some(RemoteKind::NativeSsh)),
                ]
            } else {
                CASES.to_vec()
            };
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(1200.), px(660.)),
                            cx,
                        ))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("SSH tab visual acceptance".into()),
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
                            for (index, (name, agent, remote)) in cases.iter().enumerate() {
                                let (view, mut stream) =
                                    quiet_test_pane(index as u64 + 1, window, cx);
                                if let Some(kind) = remote {
                                    DaemonMsg::RemoteContext(Some(RemoteContext {
                                        kind: *kind,
                                        argv: vec!["ssh".into(), "dev-box".into()],
                                        target: if (options.auto_names && matches!(index, 1 | 2 | 5)) || (!options.auto_names && index == 3) { "prod-web" } else { "dev-box" }
                                            .into(),
                                    }))
                                    .encode(&mut stream)
                                    .unwrap();
                                }
                                DaemonMsg::Agent(*agent).encode(&mut stream).unwrap();
                                if agent.is_some() {
                                    DaemonMsg::AgentStatus(Some(AgentSessionState {
                                        status: if screen_replay { AgentStatus::Idle } else { AgentStatus::Working },
                                        rich: !screen_replay,
                                        ..Default::default()
                                    }))
                                    .encode(&mut stream)
                                    .unwrap();
                                }
                                let output = if let Some(path) = &options.claude_question {
                                    let text = std::fs::read_to_string(path).expect("Claude question screen");
                                    format!("\x1b]0;{name}\x07\x1b[2J\x1b[H{}", text.replace('\n', "\r\n"))
                                } else if options.agent_replay {
                                    format!("\x1b]0;{name}\x07{}", agy_screen("⠿ The structure of the `view-grouped-container` section is being mapped out.", "Press up to edit queued messages"))
                                } else { format!(
                                    "\x1b]0;{name}\x07\x1b[1mSSH tab visual acceptance\x1b[0m\r\n\r\n\
                                     Production native UI, identical fixture data.\r\n\
                                     No network SSH connection is opened by this fixture.\r\n\r\n\
                                     Local: terminal / Codex / Claude Code\r\n\
                                     SSH:   terminal / Codex / Claude Code\r\n\r\n\
                                     Active session: Fix login (SSH, detected Codex)\r\n\r\n\
                                     \x1b[36mdev@dev-box\x1b[0m:/srv/api $ codex\r\n"
                                ) };
                                DaemonMsg::Output(output.into_bytes()).encode(&mut stream).unwrap();
                                let mut tab = Tab::new(Pane::leaf(PaneSlot::Ready(view.clone())));
                                if options.auto_names && index == 5 {
                                    tab.name = Some("Production logs".into());
                                }
                                app.tabs.push(tab);
                                views.push(view);
                                streams.push(stream);
                            }
                            app.active = if options.attention_badge { 1 } else if screen_replay { 0 } else { 4 };
                            app.focus_active(window, cx);
                            cx.notify();
                        });
                        if options.agent_replay || options.attention_badge {
                            replay_stream = Some(streams[0].try_clone().unwrap());
                        }
                        owner_entity = Some(owner.clone());
                        let fixture = cx.new(|_| Fixture { owner, _streams: streams });
                        cx.new(|cx| Root::new(fixture, window, cx))
                    },
                )
                .unwrap();
            cx.activate(true);
            let owner = owner_entity.unwrap();
            if options.attention_badge {
                let workspace = owner.read(cx).workspace;
                crate::ui::windows::WindowRegistry::register(cx, workspace, handle.into(), owner.downgrade());
            }
            cx.spawn(async move |cx| {
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                loop {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    let ready = views.iter().zip(cases.iter()).all(|(view, (name, agent, remote))| {
                        view.read_with(cx, |view, _| {
                            view.title == *name && view.agent() == *agent
                                && view.remote_context().map(|r| r.kind) == *remote
                        })
                    });
                    if ready { break; }
                    assert!(std::time::Instant::now() < deadline, "SSH tab fixture did not settle");
                }
                cx.update_window(handle.into(), |_, window, cx| {
                    owner.update(cx, |app, cx| app.focus_active(window, cx));
                    window.refresh();
                    let _ = window.draw(cx);
                }).unwrap();
                cx.background_executor().timer(Duration::from_secs(2)).await;
                cx.update_window(handle.into(), |_, window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                    window.set_window_title(if options.claude_question.is_some() { "Claude question replay" } else if options.agent_replay { "AGY screen replay" } else { "SSH tab visual acceptance" });
                    #[cfg(target_os = "macos")]
                    window.render_to_image().unwrap().save(&options.output).unwrap();
                    #[cfg(not(target_os = "macos"))]
                    let _ = &options.output;
                }).unwrap();
                eprintln!("NATIVE_SSH_TABS_READY");
                if options.attention_badge {
                    let mut stream = replay_stream.take().unwrap();
                    DaemonMsg::AgentStatus(Some(AgentSessionState {
                        status: AgentStatus::Done,
                        rich: true,
                        ..Default::default()
                    })).encode(&mut stream).unwrap();
                    cx.background_executor().timer(Duration::from_secs(3)).await;
                    eprintln!("NATIVE_ATTENTION_BADGE_UNREAD");
                    // The control file lets the screenshot operator acknowledge the
                    // actual result without typing commands into the fixture UI.
                    let acknowledge = options.output.with_extension("read");
                    let deadline = std::time::Instant::now() + Duration::from_secs(options.hold_seconds.unwrap_or(120));
                    while !acknowledge.exists() && std::time::Instant::now() < deadline {
                        cx.background_executor().timer(Duration::from_millis(200)).await;
                    }
                    cx.update_window(handle.into(), |_, window, cx| {
                        owner.update(cx, |app, cx| {
                            app.active = 0;
                            app.focus_active(window, cx);
                        });
                        window.activate_window();
                        window.refresh();
                    }).unwrap();
                    eprintln!("NATIVE_ATTENTION_BADGE_READ");
                }
                if let Some(mut stream) = replay_stream {
                    // Exercise the normal daemon reader and UI status merge.
                    // This is a replay of the reported screen shape, not a model request.
                    for spinner in ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"].iter().cycle().take(600) {
                        DaemonMsg::Output(agy_screen(&format!("{spinner} The structure of the `view-grouped-container` section is being mapped out."), "Press up to edit queued messages").into_bytes()).encode(&mut stream).unwrap();
                        cx.background_executor().timer(Duration::from_millis(400)).await;
                    }
                    DaemonMsg::Output(agy_screen("Run this command?", "↑/↓ Navigate · tab Amend · f full diff\nesc to cancel").into_bytes()).encode(&mut stream).unwrap();
                    cx.background_executor().timer(Duration::from_secs(12)).await;
                    DaemonMsg::Output(agy_screen("⠋ Reviewing the final response", "Press up to edit queued messages").into_bytes()).encode(&mut stream).unwrap();
                    cx.background_executor().timer(Duration::from_secs(12)).await;
                    DaemonMsg::Output(agy_screen("The structure has been mapped out. Turn complete.", "? for shortcuts · 2 tasks").into_bytes()).encode(&mut stream).unwrap();
                }
                cx.background_executor()
                    .timer(Duration::from_secs(options.hold_seconds.unwrap_or(12)))
                    .await;
                // The platform input handler owns the focused terminal. Release
                // it while the window can still draw, before GPUI checks leaks.
                cx.update_window(handle.into(), |_, window, cx| {
                    window.blur();
                    window.refresh();
                    let _ = window.draw(cx);
                    window.remove_window();
                }).unwrap();
                drop(views);
                drop(owner);
                cx.background_executor().timer(Duration::from_millis(100)).await;
                cx.update(|cx| cx.quit());
            }).detach();
        });
}

fn agy_screen(activity: &str, footer: &str) -> String {
    format!(
        "\x1b[2J\x1b[HAGY SCREEN REPLAY — no model request\n\n\
        Reproduces the reported spinner, tool history and queued-input layout.\n\n\
        > Review the workflow structure\n\n\
        ○ Thought for 1m 50s\n\
        ● Explored 1 file\n\
        ● Ran a validation command\n\
        ● Explored 1 file\n\
        ● Killed 1 task\n\
        {activity}\n\
        └ Tip: Use /settings to configure your environment.\n\n\
        ───────────────────────────────────────────────────────────────\n\
        > add emoji\n\
        > \n\
        ───────────────────────────────────────────────────────────────\n\
        Gemini 3.8 Flash · high\n\
        {footer}\n"
    )
    .replace('\n', "\r\n")
}

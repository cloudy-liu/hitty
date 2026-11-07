//! Native badge acceptance: deterministic Agent reports, real UI interactions.
//! The control file changes Agent data only; focus and menus stay under UI control.

use gpui::{prelude::*, *};
use gpui_component::Root;
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

use crate::{
    core::{
        cli_agent::{AgentSessionState, AgentStatus, CLIAgent},
        config::{Config, NotifyMode, SidebarGrouping, TabBarPosition},
        session::{Session, WorkspaceId, WorkspaceStore},
    },
    daemon::{protocol::DaemonMsg, transport::Stream},
    terminal::view::{TerminalView, quiet_test_pane},
    ui::{
        app::{Tab, Tty7App},
        pane::{Pane, PaneSlot},
        windows::WindowRegistry,
    },
};

#[derive(Deserialize)]
struct Options {
    control: PathBuf,
    state: PathBuf,
    #[serde(default)]
    split: bool,
    #[serde(default)]
    top: bool,
    #[serde(default)]
    tray_disabled: bool,
    #[serde(default)]
    dark: bool,
}

#[derive(Deserialize)]
struct Reports {
    sequence: u64,
    reports: Vec<(usize, usize, AgentStatus)>,
}

struct NativeWindow {
    handle: WindowHandle<Root>,
    panes: Vec<Entity<TerminalView>>,
    streams: Vec<Stream>,
    title: String,
}

fn report(stream: &mut Stream, status: AgentStatus, window: usize, pane: usize) {
    DaemonMsg::AgentStatus(Some(AgentSessionState {
        status,
        rich: true,
        session_id: Some(format!("native-badge-{window}-{pane}")),
        ..Default::default()
    }))
    .encode(stream)
    .unwrap();
}

fn open(index: usize, split: bool, cx: &mut App) -> NativeWindow {
    let title = format!("ctty7 badge acceptance {}", index + 1);
    let mut built = None;
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1120.), px(640.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.clone().into()),
                    ..Default::default()
                }),
                show: true,
                focus: true,
                ..Default::default()
            },
            |window, cx| {
                crate::ui::theme::apply_theme(Some(window), cx);
                let owner = cx.new(|cx| {
                    Tty7App::with_session(
                        Some(WorkspaceId::new()),
                        Some(Session::default()),
                        window,
                        cx,
                    )
                });
                let mut panes = Vec::new();
                let mut streams = Vec::new();
                owner.update(cx, |app, cx| {
                    for (pane, (name, agent)) in [
                        ("Agent A", Some(CLIAgent::Claude)),
                        ("Agent B", Some(CLIAgent::Claude)),
                        ("Other terminal", None),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let (view, mut stream) =
                            quiet_test_pane((index * 10 + pane + 1) as u64, window, cx);
                        DaemonMsg::Agent(agent).encode(&mut stream).unwrap();
                        if agent.is_some() {
                            report(&mut stream, AgentStatus::Working, index, pane);
                        }
                        DaemonMsg::Output(
                            format!(
                                "\x1b]0;{name}\x07\x1b[1m{name}\x1b[0m\r\n\r\n\
                         Native badge acceptance window {}.\r\n\
                         Fixed Agent reports; no model, shell, or network request.\r\n\
                         Focus, minimize, tabs and menus are controlled through LCU.\r\n",
                                index + 1,
                            )
                            .into_bytes(),
                        )
                        .encode(&mut stream)
                        .unwrap();
                        app.tabs
                            .push(Tab::new(Pane::leaf(PaneSlot::Ready(view.clone()))));
                        panes.push(view);
                        streams.push(stream);
                    }
                    if split {
                        let b = app.tabs.remove(1);
                        let a = std::mem::replace(&mut app.tabs[0].pane, Pane::Empty);
                        app.tabs[0].pane = Pane::split_node(Axis::Horizontal, 0.5, a, b.pane);
                    }
                    app.active = app.tabs.len() - 1;
                    app.focus_active(window, cx);
                    cx.notify();
                });
                built = Some((owner.clone(), panes, streams));
                cx.new(|cx| Root::new(owner, window, cx))
            },
        )
        .unwrap();
    let (owner, panes, streams) = built.unwrap();
    WindowRegistry::register(
        cx,
        owner.read(cx).workspace,
        handle.into(),
        owner.downgrade(),
    );
    NativeWindow {
        handle,
        panes,
        streams,
        title,
    }
}

pub(crate) fn run(path: &std::ffi::OsStr) {
    let options: Options = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    #[cfg(windows)]
    {
        // Match normal startup's taskbar identity without creating or
        // repointing the user's Start Menu shortcut from a test executable.
        let exe = std::env::current_exe().unwrap();
        let app_id = windows::core::HSTRING::from(crate::core::aumid::taskbar_app_id(&exe));
        unsafe {
            windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&app_id).unwrap();
        }
    }
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
            config.sidebar_width = 260.;
            config.sidebar_grouping = SidebarGrouping::None;
            config.tab_bar_position = if options.top {
                TabBarPosition::Top
            } else {
                TabBarPosition::Left
            };
            config.show_tray_icon = !options.tray_disabled;
            config.notify_on_command_finish = NotifyMode::Never;
            config.agent_notification_sound = NotifyMode::Never;
            cx.set_global(config);
            WorkspaceStore::init(cx);
            WindowRegistry::init(cx);
            crate::ui::presets::load_registry(cx);
            crate::ui::keymap::init(cx);
            let mut windows = vec![open(0, options.split, cx), open(1, false, cx)];
            cx.activate(true);
            cx.spawn(async move |cx| {
                let mut sequence = 0;
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(150))
                        .await;
                    if let Ok(bytes) = std::fs::read(&options.control)
                        && let Ok(reports) = serde_json::from_slice::<Reports>(&bytes)
                        && reports.sequence != sequence
                    {
                        for (window, pane, status) in reports.reports {
                            report(&mut windows[window].streams[pane], status, window, pane);
                        }
                        sequence = reports.sequence;
                    }
                    let mut snapshots = Vec::new();
                    for native in &windows {
                        if let Ok(snapshot) =
                            cx.update_window(native.handle.into(), |_, window, cx| {
                                window.set_window_title(&native.title);
                                serde_json::json!({
                                    "active": window.is_window_active(),
                                    "panes": native.panes.iter().map(|pane| {
                                        let view = pane.read(cx);
                                        serde_json::json!({
                                            "status": view.agent_status(),
                                            "unread": view.agent_result_unread(),
                                            "focused": view.focus_handle.is_focused(window),
                                        })
                                    }).collect::<Vec<_>>()
                                })
                            })
                        {
                            snapshots.push(snapshot);
                        }
                    }
                    let attention = cx.update(|cx| crate::ui::tray::app_snapshot(cx).attention());
                    let snapshot = serde_json::json!({
                        "sequence": sequence, "attention": attention, "windows": snapshots,
                    });
                    std::fs::write(
                        &options.state,
                        serde_json::to_vec_pretty(&snapshot).unwrap(),
                    )
                    .unwrap();
                }
            })
            .detach();
        });
}

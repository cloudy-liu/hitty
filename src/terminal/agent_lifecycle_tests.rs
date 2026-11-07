use super::*;
use crate::core::cli_agent::{AgentSessionState, AgentStatus};
use crate::daemon::protocol::DaemonMsg;
use crate::terminal::agent_notification::RecordingDelivery;
use gpui::{Entity, TestAppContext, WindowHandle};

fn harness(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<gpui_component::Root>,
    Entity<TerminalView>,
    crate::daemon::transport::Stream,
) {
    crate::core::config::pin_test_config_dir();
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut config = Config::default();
        config.notify_on_command_finish = NotifyMode::Always;
        config.agent_notification_sound = NotifyMode::Always;
        cx.set_global(config);
        cx.set_global(RecordingDelivery::default());
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let (view, stream) = quiet_test_pane(135, window, cx);
        built = Some((view.clone(), stream));
        gpui_component::Root::new(view, window, cx)
    });
    let (view, stream) = built.unwrap();
    cx.run_until_parked();
    (window, view, stream)
}

#[gpui::test]
fn agent_lifecycle_osc_progress_drives_live_and_replayed_status(cx: &mut TestAppContext) {
    use crate::core::cli_agent::CLIAgent;

    let (window, view, mut daemon) = harness(cx);
    DaemonMsg::Agent(Some(CLIAgent::Grok))
        .encode(&mut daemon)
        .unwrap();
    // The report crosses output frames, as it can cross PTY reads or SSH packets.
    DaemonMsg::Output(b"\x1b]9;4;1;".to_vec())
        .encode(&mut daemon)
        .unwrap();
    for (frame, expected) in [
        (
            DaemonMsg::Output(b"-1\x1b\\".to_vec()),
            AgentStatus::Working,
        ),
        (
            DaemonMsg::Output(b"\x1b]9;4;0;0\x07".to_vec()),
            AgentStatus::Done,
        ),
        (
            DaemonMsg::Snapshot(b"\x1b]9;4;1;-1\x07".to_vec()),
            AgentStatus::Working,
        ),
    ] {
        frame.encode(&mut daemon).unwrap();
        let mut observed = false;
        for _ in 0..200 {
            cx.run_until_parked();
            observed = window
                .update(cx, |_, window, cx| {
                    view.update(cx, |pane, cx| {
                        pane.poll_foreground(window, cx);
                        pane.agent_status() == Some(expected)
                    })
                })
                .unwrap();
            if observed {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            observed,
            "OSC progress must reach the displayed {expected:?} status"
        );
    }
}

fn report(
    status: AgentStatus,
    daemon: &mut crate::daemon::transport::Stream,
    window: WindowHandle<gpui_component::Root>,
    view: &Entity<TerminalView>,
    cx: &mut TestAppContext,
) {
    report_session(
        Some(AgentSessionState {
            status,
            rich: true,
            session_id: Some("lifecycle".into()),
            launch_argv: Some(vec!["claude".into()]),
            unstarted: false,
            message: None,
            cwd: None,
            activity: 0,
        }),
        daemon,
        window,
        view,
        cx,
    );
}

fn report_session(
    session: Option<AgentSessionState>,
    daemon: &mut crate::daemon::transport::Stream,
    window: WindowHandle<gpui_component::Root>,
    view: &Entity<TerminalView>,
    cx: &mut TestAppContext,
) {
    DaemonMsg::AgentStatus(session.clone())
        .encode(daemon)
        .unwrap();
    for _ in 0..200 {
        let ready = window
            .update(cx, |_, window, cx| {
                view.update(cx, |pane, cx| {
                    if pane.agent_session() != session {
                        return false;
                    }
                    pane.poll_foreground(window, cx);
                    true
                })
            })
            .unwrap();
        if ready {
            cx.run_until_parked();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("Agent report did not reach the view");
}

#[gpui::test]
fn returning_to_another_tab_clears_the_app_badge_but_keeps_the_result_unread(
    cx: &mut TestAppContext,
) {
    use crate::core::cli_agent::CLIAgent;
    use crate::core::session::{Session, WorkspaceId, WorkspaceStore};
    use crate::ui::app::{Tab, Tty7App};
    use crate::ui::pane::{Pane, PaneSlot};
    use crate::ui::windows::WindowRegistry;

    crate::core::config::pin_test_config_dir();
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut config = Config::default();
        config.notify_on_command_finish = NotifyMode::Never;
        config.agent_notification_sound = NotifyMode::Never;
        cx.set_global(config);
        cx.set_global(RecordingDelivery::default());
        WorkspaceStore::init(cx);
        WindowRegistry::init(cx);
        crate::ui::keymap::init(cx);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let app = cx.new(|cx| {
            Tty7App::with_session(
                Some(WorkspaceId::new()),
                Some(Session::default()),
                window,
                cx,
            )
        });
        let (agent, mut daemon) = quiet_test_pane(301, window, cx);
        let (other, _other_daemon) = quiet_test_pane(302, window, cx);
        DaemonMsg::Agent(Some(CLIAgent::Claude))
            .encode(&mut daemon)
            .unwrap();
        app.update(cx, |app, cx| {
            app.tabs
                .push(Tab::new(Pane::leaf(PaneSlot::Ready(agent.clone()))));
            app.tabs
                .push(Tab::new(Pane::leaf(PaneSlot::Ready(other.clone()))));
            app.active = 1;
            app.focus_active(window, cx);
        });
        built = Some((app.clone(), agent, daemon));
        gpui_component::Root::new(app, window, cx)
    });
    let (app, agent, mut daemon) = built.unwrap();
    app.update(cx, |app, cx| {
        let weak = cx.entity().downgrade();
        WindowRegistry::register(cx, app.workspace, window.into(), weak);
    });
    cx.run_until_parked();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.deactivate_window();
    report(AgentStatus::Waiting, &mut daemon, window, &agent, cx);
    cx.update(|cx| {
        assert!(
            !crate::ui::tray::app_snapshot(cx).attention(),
            "a restored waiting snapshot is not a new background event"
        );
    });
    report(AgentStatus::Working, &mut daemon, window, &agent, cx);
    report(AgentStatus::Done, &mut daemon, window, &agent, cx);
    cx.update(|cx| {
        assert!(agent.read(cx).agent_result_unread());
        assert!(crate::ui::tray::app_snapshot(cx).attention());
    });
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            agent.read(cx).agent_result_unread(),
            "another tab is still unread"
        );
        assert!(
            !crate::ui::tray::app_snapshot(cx).attention(),
            "returning to the app must clear its badge without visiting the result"
        );
    });

    visual.deactivate_window();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(agent.read(cx).agent_result_unread());
        assert!(
            !crate::ui::tray::app_snapshot(cx).attention(),
            "an acknowledged result must not reappear when leaving again"
        );
    });

    report(AgentStatus::Working, &mut daemon, window, &agent, cx);
    report(AgentStatus::Waiting, &mut daemon, window, &agent, cx);
    cx.update(|cx| assert!(crate::ui::tray::app_snapshot(cx).attention()));

    let another_window = cx.add_window(|window, cx| {
        let app = cx.new(|cx| {
            Tty7App::with_session(
                Some(WorkspaceId::new()),
                Some(Session::default()),
                window,
                cx,
            )
        });
        gpui_component::Root::new(app, window, cx)
    });
    another_window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    assert!(
        another_window
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap()
    );
    assert!(
        !window
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap()
    );
    cx.update(|cx| {
        assert_eq!(agent.read(cx).agent_status(), Some(AgentStatus::Waiting));
        assert!(
            !crate::ui::tray::app_snapshot(cx).attention(),
            "activating any ctty7 window acknowledges the background event"
        );
    });
    report(AgentStatus::Working, &mut daemon, window, &agent, cx);
    report(AgentStatus::Done, &mut daemon, window, &agent, cx);
    cx.update(|cx| {
        assert!(
            !crate::ui::tray::app_snapshot(cx).attention(),
            "another active ctty7 window prevents a background badge"
        );
    });
    let mut second_visual = gpui::VisualTestContext::from_window(another_window.into(), cx);
    second_visual.deactivate_window();
    report(AgentStatus::Working, &mut daemon, window, &agent, cx);
    report(AgentStatus::Done, &mut daemon, window, &agent, cx);
    cx.update(|cx| {
        assert!(
            crate::ui::tray::app_snapshot(cx).attention(),
            "a later background event creates a fresh reminder"
        );
    });
}

#[gpui::test]
fn agent_lifecycle_initial_waiting_is_a_silent_baseline(cx: &mut TestAppContext) {
    let (window, view, mut daemon) = harness(cx);
    report(AgentStatus::Waiting, &mut daemon, window, &view, cx);
    cx.update(|cx| {
        assert_eq!(view.read(cx).agent_status(), Some(AgentStatus::Waiting));
        assert!(
            cx.global::<RecordingDelivery>().desktops.is_empty(),
            "restored waiting is not a live input event"
        );
        assert_eq!(cx.global::<RecordingDelivery>().sounds, 0);
    });
}

#[gpui::test]
fn agent_lifecycle_initial_done_and_idle_do_not_alert(cx: &mut TestAppContext) {
    let (window, view, mut daemon) = harness(cx);
    for status in [AgentStatus::Done, AgentStatus::Idle, AgentStatus::Done] {
        report(status, &mut daemon, window, &view, cx);
    }
    cx.update(|cx| {
        assert!(cx.global::<RecordingDelivery>().desktops.is_empty());
        assert_eq!(cx.global::<RecordingDelivery>().sounds, 0);
    });
}

#[gpui::test]
fn agent_lifecycle_new_session_does_not_finish_previous_work(cx: &mut TestAppContext) {
    let (window, view, mut daemon) = harness(cx);
    report(AgentStatus::Working, &mut daemon, window, &view, cx);
    report_session(None, &mut daemon, window, &view, cx);
    let mut snapshot = AgentSessionState {
        status: AgentStatus::Done,
        rich: true,
        session_id: Some("different-session".into()),
        launch_argv: Some(vec!["claude".into()]),
        unstarted: false,
        message: None,
        cwd: None,
        activity: 0,
    };
    report_session(Some(snapshot.clone()), &mut daemon, window, &view, cx);
    cx.update(|cx| assert!(cx.global::<RecordingDelivery>().desktops.is_empty()));
    snapshot.status = AgentStatus::Working;
    report_session(Some(snapshot.clone()), &mut daemon, window, &view, cx);
    snapshot.status = AgentStatus::Done;
    report_session(Some(snapshot), &mut daemon, window, &view, cx);
    cx.update(|cx| assert_eq!(cx.global::<RecordingDelivery>().desktops.len(), 1));
}

#[gpui::test]
fn agent_lifecycle_reconnect_baselines_old_done_and_waiting(cx: &mut TestAppContext) {
    let (window, view, mut daemon) = harness(cx);
    for snapshot in [AgentStatus::Done, AgentStatus::Waiting] {
        report(AgentStatus::Working, &mut daemon, window, &view, cx);
        #[cfg(unix)]
        let (client, next_daemon) = std::os::unix::net::UnixStream::pair().unwrap();
        #[cfg(windows)]
        let (client, next_daemon) = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            (client, listener.accept().unwrap().0)
        };
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |pane, cx| {
                    pane.adopt_relink(
                        client,
                        Vec::new(),
                        &crate::terminal::PaneRoute::Local,
                        TermSize::new(80, 24),
                        8,
                        16,
                        cx,
                    )
                    .unwrap();
                })
            })
            .unwrap();
        daemon = next_daemon;
        report(snapshot, &mut daemon, window, &view, cx);
    }
    cx.update(|cx| {
        assert!(cx.global::<RecordingDelivery>().desktops.is_empty());
        assert_eq!(cx.global::<RecordingDelivery>().sounds, 0);
    });
    report(AgentStatus::Working, &mut daemon, window, &view, cx);
    report(AgentStatus::Done, &mut daemon, window, &view, cx);
    cx.update(|cx| assert_eq!(cx.global::<RecordingDelivery>().desktops.len(), 1));
}

#[gpui::test]
fn agent_lifecycle_process_exit_does_not_repeat_a_completed_turn(cx: &mut TestAppContext) {
    use crate::core::cli_agent::CLIAgent;
    let (window, view, mut daemon) = harness(cx);
    screen(
        "Codex",
        Some(AgentStatus::Idle),
        &mut daemon,
        window,
        &view,
        cx,
    );
    screen(
        "⠋ Codex",
        Some(AgentStatus::Working),
        &mut daemon,
        window,
        &view,
        cx,
    );
    screen(
        "Codex",
        Some(AgentStatus::Done),
        &mut daemon,
        window,
        &view,
        cx,
    );
    DaemonMsg::Prompt {
        active: true,
        at_prompt: true,
        last_exit: Some(0),
    }
    .encode(&mut daemon)
    .unwrap();
    for _ in 0..200 {
        if cx.update(|cx| view.read(cx).terminal.at_prompt()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |pane, cx| {
                assert!(pane.terminal.at_prompt());
                assert_eq!(pane.running_agent, Some(CLIAgent::Codex));
                pane.poll_foreground(window, cx);
            })
        })
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(cx.global::<RecordingDelivery>().desktops.len(), 1));
}

fn screen(
    title: &str,
    status: Option<AgentStatus>,
    daemon: &mut crate::daemon::transport::Stream,
    window: WindowHandle<gpui_component::Root>,
    view: &Entity<TerminalView>,
    cx: &mut TestAppContext,
) {
    use crate::core::cli_agent::CLIAgent;
    DaemonMsg::Agent(Some(CLIAgent::Codex))
        .encode(daemon)
        .unwrap();
    DaemonMsg::Output(format!("\x1b]0;{title}\x07").into_bytes())
        .encode(daemon)
        .unwrap();
    if status == Some(AgentStatus::Idle) {
        // Codex's plain title carries no state. Feed the ready footer that
        // the screen manifest uses as positive idle evidence.
        DaemonMsg::Output(b"\r\n\x1b[2K\x1b[1G\xE2\x80\xBA\r\n  ? for shortcuts\r\n".to_vec())
            .encode(daemon)
            .unwrap();
    }
    for _ in 0..200 {
        cx.run_until_parked();
        let ready = window
            .update(cx, |_, window, cx| {
                view.update(cx, |pane, cx| {
                    let current = pane.pending_title.as_deref().unwrap_or(&pane.title);
                    if current != title || pane.agent() != Some(CLIAgent::Codex) {
                        return false;
                    }
                    pane.poll_foreground(window, cx);
                    pane.agent_status() == status
                })
            })
            .unwrap();
        if ready {
            cx.run_until_parked();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let actual = cx.update(|cx| {
        let pane = view.read(cx);
        (
            pane.title.clone(),
            pane.pending_title.clone(),
            pane.agent(),
            pane.agent_status(),
        )
    });
    panic!("screen state {status:?} did not reach the view: {actual:?}");
}

#[gpui::test]
fn agent_lifecycle_screen_turns_alert_without_rich_hooks(cx: &mut TestAppContext) {
    let (window, view, mut daemon) = harness(cx);
    for (title, status) in [
        ("Codex", AgentStatus::Idle),
        ("⠋ Codex", AgentStatus::Working),
        ("Action Required", AgentStatus::Waiting),
        ("Action Required", AgentStatus::Waiting),
        ("⠋ Codex", AgentStatus::Working),
        ("Codex", AgentStatus::Done),
        ("Codex", AgentStatus::Done),
    ] {
        screen(title, Some(status), &mut daemon, window, &view, cx);
    }
    cx.update(|cx| {
        let delivered = cx.global::<RecordingDelivery>();
        assert_eq!(
            delivered.desktops.len(),
            2,
            "one input and one completion alert"
        );
        assert!(
            delivered
                .desktops
                .iter()
                .all(|(_, _, pane, silent)| *pane == view.entity_id() && *silent)
        );
        assert_eq!(delivered.sounds, 2);
        assert_eq!(view.read(cx).agent_status(), Some(AgentStatus::Done));
    });
}

#[gpui::test]
fn agent_lifecycle_waiting_rearms_only_after_real_work(cx: &mut TestAppContext) {
    let (window, view, mut daemon) = harness(cx);
    for status in [
        AgentStatus::Idle,
        AgentStatus::Working,
        AgentStatus::Waiting,
        AgentStatus::Idle,
        AgentStatus::Waiting,
    ] {
        report(status, &mut daemon, window, &view, cx);
    }
    cx.update(|cx| {
        assert_eq!(
            cx.global::<RecordingDelivery>().desktops.len(),
            1,
            "idle did not resume work, so the waiting episode is unchanged"
        )
    });
    report_session(None, &mut daemon, window, &view, cx);
    report(AgentStatus::Waiting, &mut daemon, window, &view, cx);
    cx.update(|cx| {
        assert_eq!(
            cx.global::<RecordingDelivery>().desktops.len(),
            1,
            "a missing report did not resume work"
        )
    });
    report(AgentStatus::Working, &mut daemon, window, &view, cx);
    report(AgentStatus::Waiting, &mut daemon, window, &view, cx);
    report(AgentStatus::Working, &mut daemon, window, &view, cx);
    report_session(None, &mut daemon, window, &view, cx);
    report(AgentStatus::Done, &mut daemon, window, &view, cx);
    report_session(None, &mut daemon, window, &view, cx);
    report(AgentStatus::Done, &mut daemon, window, &view, cx);
    cx.update(|cx| {
        assert_eq!(
            cx.global::<RecordingDelivery>().desktops.len(),
            3,
            "two waiting episodes and one observed completion"
        );
        assert_eq!(cx.global::<RecordingDelivery>().sounds, 3);
    });
}

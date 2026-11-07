use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tty7_core::client::{ControlClient, PaneClient};
use tty7_core::core::machine::{LayoutDelta, PaneSeed};
use tty7_core::daemon::control::{ControlEvent, ControlHello, ControlRequest, ReplyOk, feature};
use tty7_core::daemon::protocol::{DaemonMsg, PROTOCOL_VERSION, ShellSpec, WinSize};

const READY_WITHIN: Duration = Duration::from_secs(30);
const STREAM_WITHIN: Duration = Duration::from_secs(30);

struct Daemon {
    child: Child,
    dir: tempfile::TempDir,
}

impl Daemon {
    fn start() -> Daemon {
        let dir = tempfile::TempDir::new().unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_tty7-server"))
            .arg("--daemon")
            .arg("--config-dir")
            .arg(dir.path())
            .env("TTY7_DATA_DIR", dir.path())
            .env("TTY7_CONTROL_SOCK", dir.path().join("control.sock"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start tty7-server --daemon");
        let daemon = Daemon { child, dir };
        daemon.await_ready();
        daemon
    }

    fn pane_endpoint(&self) -> PathBuf {
        let file = if cfg!(windows) {
            "daemon.port"
        } else {
            "daemon.sock"
        };
        self.dir.path().join(file)
    }

    fn control_endpoint(&self) -> PathBuf {
        let file = if cfg!(windows) {
            "control.port"
        } else {
            "control.sock"
        };
        self.dir.path().join(file)
    }

    fn panes(&self) -> PaneClient {
        PaneClient::at(self.pane_endpoint())
    }

    fn control(&self, name: &str) -> ControlClient {
        ControlClient::connect_at(&self.control_endpoint(), &hello(name))
            .expect("control handshake with the spawned server")
    }

    fn await_ready(&self) {
        let deadline = Instant::now() + READY_WITHIN;
        loop {
            let control_up =
                ControlClient::connect_at(&self.control_endpoint(), &hello("probe")).is_ok();
            let panes_up = self.panes().version().is_ok();
            if control_up && panes_up {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "tty7-server did not open its endpoints within {READY_WITHIN:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hello(name: &str) -> ControlHello {
    ControlHello::host_rpc(name, name)
}

fn size() -> WinSize {
    WinSize {
        cols: 100,
        rows: 30,
        cell_w: 8,
        cell_h: 16,
    }
}

fn one_shot_shell(command: &str) -> ShellSpec {
    if cfg!(windows) {
        ShellSpec {
            program: "cmd.exe".into(),
            args: vec!["/d".into(), "/c".into(), command.into()],
            args_are_tty7_defaults: false,
        }
    } else {
        ShellSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), command.into()],
            args_are_tty7_defaults: false,
        }
    }
}

fn interactive_shell() -> ShellSpec {
    if cfg!(windows) {
        ShellSpec {
            program: "cmd.exe".into(),
            args: vec!["/d".into()],
            args_are_tty7_defaults: false,
        }
    } else {
        ShellSpec {
            program: "/bin/sh".into(),
            args: Vec::new(),
            args_are_tty7_defaults: false,
        }
    }
}

fn seed(pane: u64) -> PaneSeed {
    PaneSeed {
        pane,
        cwd: Some("/home/me/proj".into()),
        ssh_spec: None,
        agent: None,
        shell: None,
    }
}

fn collect_until(
    session: &mut tty7_core::client::PaneSession,
    marker: &[u8],
) -> (Vec<u8>, Option<Option<i32>>) {
    let mut seen: Vec<u8> = Vec::new();
    loop {
        match session.recv() {
            Ok(DaemonMsg::Output(bytes)) | Ok(DaemonMsg::Snapshot(bytes)) => {
                seen.extend_from_slice(&bytes);
                if windows_contain(&seen, marker) {
                    return (seen, None);
                }
            }
            Ok(DaemonMsg::Exited { code }) => return (seen, Some(code)),
            Ok(_) => {}
            Err(e) => panic!(
                "pane stream ended early: {e}; saw {:?}",
                String::from_utf8_lossy(&seen)
            ),
        }
    }
}

fn drain_until_exit(session: &mut tty7_core::client::PaneSession) -> Vec<u8> {
    let mut seen: Vec<u8> = Vec::new();
    loop {
        match session.recv() {
            Ok(DaemonMsg::Output(bytes)) | Ok(DaemonMsg::Snapshot(bytes)) => {
                seen.extend_from_slice(&bytes);
            }
            Ok(DaemonMsg::Exited { .. }) => return seen,
            Ok(_) => {}
            Err(e) => panic!(
                "pane stream ended before Exited: {e}; saw {:?}",
                String::from_utf8_lossy(&seen)
            ),
        }
    }
}

fn windows_contain(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn the_pane_daemon_reports_its_dialect() {
    let daemon = Daemon::start();
    let version = daemon.panes().version().expect("query the version");
    assert_eq!(version.protocol, PROTOCOL_VERSION);
}

#[test]
fn control_requests_build_the_tree_and_events_reach_the_other_client() {
    let daemon = Daemon::start();
    let writer = daemon.control("writer");
    assert!(
        writer.hello().has_feature(feature::MACHINE_TREE),
        "features were {:?}",
        writer.hello().features
    );

    let watcher = daemon.control("watcher");
    watcher
        .request(ControlRequest::Ping)
        .expect("the watcher is live before the writer acts");

    let ws = match writer
        .request(ControlRequest::WorkspaceCreate {
            name: Some("api".into()),
            workspace: None,
        })
        .expect("create a workspace")
    {
        ReplyOk::WorkspaceTree(ws) => *ws,
        other => panic!("expected WorkspaceTree, got {other:?}"),
    };
    match writer
        .request(ControlRequest::TabCreate {
            workspace: ws.id,
            at: None,
            pane: seed(1),
            tab: None,
        })
        .expect("create a tab")
    {
        ReplyOk::TabTree(_) => {}
        other => panic!("expected TabTree, got {other:?}"),
    }

    let machine = match writer
        .request(ControlRequest::MachineGet)
        .expect("fetch the machine tree")
    {
        ReplyOk::MachineTree(m) => *m,
        other => panic!("expected MachineTree, got {other:?}"),
    };
    assert_eq!(machine.workspaces.len(), 1);
    assert_eq!(machine.workspaces[0].tabs[0].root.pane_ids(), vec![1]);

    let key = ws.id.to_string();
    let deadline = Instant::now() + STREAM_WITHIN;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "the watcher never saw the WorkspaceCreated delta for {key}"
        );
        match watcher.next_event(remaining) {
            Some(ControlEvent::Layout { workspace, delta })
                if workspace == key && matches!(delta, LayoutDelta::WorkspaceCreated { .. }) =>
            {
                break;
            }
            Some(_) => {}
            None => {}
        }
    }
}

#[test]
fn a_spawned_pane_streams_its_output_and_its_exit() {
    let daemon = Daemon::start();
    let mut session = daemon
        .panes()
        .spawn(
            None,
            size(),
            Some(one_shot_shell("echo tty7_pane_roundtrip")),
            Some("client-lib-test".into()),
            None,
        )
        .expect("spawn a one-shot pane");
    assert_ne!(session.pane_id(), 0, "the daemon must name the pane");
    session
        .set_recv_timeout(Some(STREAM_WITHIN))
        .expect("bound the stream reads");

    let seen = drain_until_exit(&mut session);
    assert!(
        windows_contain(&seen, b"tty7_pane_roundtrip"),
        "output was {:?}",
        String::from_utf8_lossy(&seen)
    );
}

#[test]
fn a_one_shot_pane_reports_its_real_exit_code() {
    let daemon = Daemon::start();
    let mut session = daemon
        .panes()
        .spawn(
            None,
            size(),
            Some(one_shot_shell("exit 5")),
            Some("client-lib-test".into()),
            None,
        )
        .expect("spawn a failing one-shot pane");
    session
        .set_recv_timeout(Some(STREAM_WITHIN))
        .expect("bound the stream reads");
    loop {
        match session.recv() {
            Ok(DaemonMsg::Exited { code }) => {
                assert_eq!(code, Some(5), "the child's code must ride the Exited frame");
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("pane stream ended before Exited: {e}"),
        }
    }
}

#[test]
fn input_reaches_the_shell_and_a_reattach_replays_it() {
    let daemon = Daemon::start();
    let panes = daemon.panes();
    let mut session = panes
        .spawn(None, size(), Some(interactive_shell()), None, None)
        .expect("spawn an interactive pane");
    let pane_id = session.pane_id();
    session
        .set_recv_timeout(Some(STREAM_WITHIN))
        .expect("bound the stream reads");

    session
        .input(b"echo tty7_attach_replay\r")
        .expect("type into the pane");
    let (_, exit) = collect_until(&mut session, b"tty7_attach_replay");
    assert!(exit.is_none(), "the shell must still be running");
    session.detach().expect("detach");

    let listed = panes.list().expect("list panes");
    let entry = listed
        .iter()
        .find(|p| p.pane_id == pane_id)
        .expect("the detached pane is still listed");
    assert!(entry.alive, "the detached pane is still alive");

    let mut reattached = panes.attach(pane_id, size()).expect("reattach");
    reattached
        .set_recv_timeout(Some(STREAM_WITHIN))
        .expect("bound the replay reads");
    let (_, exit) = collect_until(&mut reattached, b"tty7_attach_replay");
    assert!(exit.is_none(), "the replayed pane is still running");

    let refused = panes
        .attach(u64::MAX, size())
        .expect_err("attaching to a pane that never existed must fail");
    assert!(
        refused.to_string().contains("no such pane"),
        "the refusal was {refused}"
    );

    // This test covers attach replay rather than the streaming connection's
    // Kill command. Close that connection explicitly, then use the one-shot
    // PaneClient path for deterministic cleanup on loaded Windows runners.
    drop(reattached);
    panes.kill(pane_id).expect("kill the pane");
    let deadline = Instant::now() + STREAM_WITHIN;
    loop {
        let listed = panes.list().expect("list panes after the kill");
        let gone = !listed.iter().any(|p| p.pane_id == pane_id && p.alive);
        if gone {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pane {pane_id} was still listed alive after Kill: {listed:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Codex's shared app-server can retain the first pane's environment. Exercise
/// the real Windows hook executable and daemon, before any prompt is submitted.
#[cfg(windows)]
#[test]
fn codex_hooks_from_a_shared_app_server_reach_their_own_pane() {
    check_codex_hook_routing(false);
}

#[cfg(windows)]
#[test]
fn ambiguous_codex_hooks_leave_existing_sessions_untouched() {
    check_codex_hook_routing(true);
}

#[cfg(windows)]
fn check_codex_hook_routing(shared_directory: bool) {
    use std::io::Write as _;
    use std::os::windows::process::CommandExt as _;
    use tty7_core::core::cli_agent::AgentStatus;

    fn wait_for_session(
        session: &mut tty7_core::client::PaneSession,
        id: Option<&str>,
        status: AgentStatus,
    ) {
        let mut seen = Vec::new();
        loop {
            match session.recv() {
                Ok(DaemonMsg::AgentStatus(Some(state)))
                    if state.session_id.as_deref() == id && state.status == status =>
                {
                    return;
                }
                Ok(message) => {
                    if seen.len() < 20 {
                        seen.push(message);
                    }
                }
                Err(error) => panic!("waiting for {id:?}/{status:?}: {error}; saw {seen:?}"),
            }
        }
    }

    let daemon = Daemon::start();
    let panes = daemon.panes();
    let first_dir = daemon.dir.path().join("ctty7");
    let second_dir = if shared_directory {
        first_dir.clone()
    } else {
        daemon.dir.path().join("plain-folder")
    };
    std::fs::create_dir(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let mut sessions = Vec::new();
    for (cwd, id) in [(&first_dir, "session-a"), (&second_dir, "")] {
        // Emit the process-detection hook from the pane's own output, then
        // leave the shell alive without starting a real Codex subscription.
        let cwd_text = cwd.to_str().unwrap().replace('\\', "/");
        let startup = format!(
            "@echo off\r\necho \x1b]777;notify;tty7://cli-agent;{{\"agent\":\"codex\",\"event\":\"session-start\",\"session_id\":\"{id}\",\"cwd\":\"{cwd_text}\"}}\x07\r\nping -n 61 127.0.0.1 > nul\r\n"
        );
        let fixture = daemon
            .dir
            .path()
            .join(format!("pane-{}.cmd", sessions.len()));
        std::fs::write(&fixture, startup).unwrap();
        let mut session = panes
            .spawn(
                Some(cwd.clone()),
                size(),
                Some(one_shot_shell(fixture.to_str().unwrap())),
                None,
                None,
            )
            .unwrap();
        session.set_recv_timeout(Some(STREAM_WITHIN)).unwrap();
        wait_for_session(
            &mut session,
            (!id.is_empty()).then_some(id),
            AgentStatus::Idle,
        );
        sessions.push(session);
    }

    let inherited_pane = sessions[0].pane_id();
    let cwd = second_dir.to_str().unwrap().replace('\\', "/");
    for (event, status) in [
        ("session-start", AgentStatus::Idle),
        ("prompt-submit", AgentStatus::Working),
    ] {
        let mut hook = Command::new(env!("CARGO_BIN_EXE_tty7-server"))
            .args(["agent-hook", "codex", event])
            .env("TTY7", "routing-test")
            .env("TTY7_CONFIG_DIR", daemon.dir.path())
            .env("TTY7_PANE", inherited_pane.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .unwrap();
        let payload =
            format!(r#"{{"session_id":"session-b","cwd":"{cwd}","prompt":"first message"}}"#);
        hook.stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        assert!(hook.wait().unwrap().success());
        wait_for_session(&mut sessions[1], Some("session-b"), status);
    }
    if shared_directory {
        let control = daemon.control("ambiguous-hooks");
        let before = control.request(ControlRequest::AgentStates).unwrap();
        let ambiguous = format!(
            r#"{{"agent":"codex","event":"session-start","source":"clear","session_id":"unknown-new-session","cwd":"{cwd}"}}"#
        );
        for named in [inherited_pane, u64::MAX] {
            panes
                .report_agent_event(named, std::process::id(), &ambiguous)
                .expect("acknowledge an unresolved Codex event without terminal fallback");
            assert_eq!(
                control.request(ControlRequest::AgentStates).unwrap(),
                before,
                "an ambiguous /new must not overwrite either existing session"
            );
        }
    }
    sessions[0]
        .set_recv_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    loop {
        match sessions[0].recv() {
            Ok(DaemonMsg::AgentStatus(Some(state))) => {
                assert_eq!(state.session_id.as_deref(), Some("session-a"));
                assert_eq!(state.cwd.as_deref(), Some(first_dir.as_path()));
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                break;
            }
            Err(error) => panic!("first pane stream: {error}"),
        }
    }
    for session in sessions {
        session.kill().unwrap();
    }
}

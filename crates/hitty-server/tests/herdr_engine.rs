//! Exercise the opt-in engine through the public pane protocol and a real hc.
//! Only terminals belonging to this temporary Hitty profile are touched.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use hitty_core::client::{PaneClient, PaneSession};
use hitty_core::daemon::protocol::{ClientMsg, DaemonMsg, ShellSpec, WinSize};
use hitty_core::daemon::transport;
use serde_json::{Value, json};

struct Core {
    child: Child,
    input: ChildStdin,
    output: std::sync::mpsc::Receiver<String>,
    id: u64,
}

impl Core {
    fn connect(binary: &Path) -> Self {
        let mut command = Command::new(binary);
        command
            .arg("connect")
            .env_remove("HC_SESSION")
            .env_remove("HC_TERMINAL_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_console(&mut command);
        let mut child = command.spawn().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (lines, output) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let mut line = String::new();
                if !matches!(stdout.read_line(&mut line), Ok(n) if n > 0)
                    || lines.send(line).is_err()
                {
                    break;
                }
            }
        });
        Self {
            input: child.stdin.take().unwrap(),
            output,
            child,
            id: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        self.id += 1;
        let request = format!(
            "{}\n",
            json!({
                "jsonrpc": "2.0", "id": self.id, "method": method, "params": params
            })
        );
        self.input
            .write_all(request.as_bytes())
            .and_then(|_| self.input.flush())
            .map_err(|error| json!({"method": method, "transport": error.to_string()}))?;
        let line = self
            .output
            .recv_timeout(Duration::from_secs(20))
            .map_err(|error| json!({"method": method, "transport": error.to_string()}))?;
        let reply: Value = serde_json::from_str(&line)
            .map_err(|error| json!({"method": method, "protocol": error.to_string()}))?;
        if reply["id"] != self.id {
            return Err(json!({"method": method, "unexpected_reply": reply}));
        }
        if let Some(error) = reply.get("error") {
            Err(error.clone())
        } else {
            Ok(reply["result"].clone())
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill(); // only this test's hc connect transport
        let _ = self.child.wait();
    }
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn installed_core() -> Option<PathBuf> {
    let mut candidates = vec![
        std::env::var_os("HITTY_HC_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| "hc".into()),
    ];
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Programs/herdr-core/hc.exe"));
    }
    candidates.into_iter().find(|path| {
        let mut command = Command::new(path);
        command.arg("--version").stdin(Stdio::null());
        hide_console(&mut command);
        command
            .output()
            .is_ok_and(|out| out.status.success() && out.stdout.starts_with(b"hc "))
    })
}

struct Instance {
    dir: tempfile::TempDir,
    child: Option<Child>,
    hc: PathBuf,
    core: Option<Core>,
}

impl Instance {
    fn new(hc: PathBuf, real: bool) -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"pane_engine":"herdr-core"}"#,
        )
        .unwrap();
        let core = real.then(|| Core::connect(&hc));
        Self {
            dir,
            child: None,
            hc,
            core,
        }
    }

    fn endpoint(&self) -> PathBuf {
        self.dir.path().join(if cfg!(windows) {
            "daemon.port"
        } else {
            "daemon.sock"
        })
    }

    fn panes(&self) -> PaneClient {
        PaneClient::at(self.endpoint())
    }

    fn start(&mut self) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hitty-server"));
        command
            .args(["--daemon", "--config-dir"])
            .arg(self.dir.path())
            .env("HITTY_DATA_DIR", self.dir.path())
            .env("HITTY_CONTROL_SOCK", self.dir.path().join("control.sock"))
            .env("HITTY_HC_BIN", &self.hc)
            .env("HC_SESSION", "hitty-test-inherited-do-not-use")
            .env("HC_TERMINAL_ID", "hitty-test-parent")
            .env_remove("HITTY_PANE_ENGINE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        self.child = Some(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.panes().version().is_err() {
            assert!(Instant::now() < deadline, "daemon readiness");
            assert!(self.child.as_mut().unwrap().try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn stop(&mut self) {
        let mut stream = transport::connect_endpoint_at(&self.endpoint()).unwrap();
        ClientMsg::Shutdown.encode(&mut stream).unwrap();
        stream.flush().unwrap();
        drop(stream);
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.child.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "daemon shutdown");
            std::thread::sleep(Duration::from_millis(20));
        }
        self.child = None;
    }

    fn terminals(&mut self) -> Vec<Value> {
        let owner = self.owner();
        self.core
            .as_mut()
            .unwrap()
            .call(
                "terminal.list",
                json!({
                    "all": true, "labels": {"hitty.owner": owner}
                }),
            )
            .unwrap()["terminals"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn owner(&self) -> String {
        format!(
            "{:016x}",
            hitty_core::host::fnv1a64(self.dir.path().to_string_lossy().as_bytes())
        )
    }

    fn spawn(&self) -> PaneSession {
        let shell = if cfg!(windows) {
            ShellSpec {
                program: "cmd.exe".into(),
                args: vec!["/d".into(), "/q".into()],
                args_are_hitty_defaults: false,
            }
        } else {
            ShellSpec {
                program: "/bin/sh".into(),
                args: vec![],
                args_are_hitty_defaults: false,
            }
        };
        self.spawn_shell(shell)
    }

    fn spawn_shell(&self, shell: ShellSpec) -> PaneSession {
        let pane = self
            .panes()
            .spawn(
                Some(self.dir.path().to_owned()),
                size(),
                Some(shell),
                None,
                None,
            )
            .unwrap();
        pane.set_recv_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        pane
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // Failure cleanup is also scoped by the profile label, never the user's core.
        let owner = self.owner();
        if let Some(core) = self.core.as_mut() {
            match core.call(
                "terminal.list",
                json!({"all": true, "labels": {"hitty.owner": owner}}),
            ) {
                Ok(result) => {
                    for terminal in result["terminals"].as_array().into_iter().flatten() {
                        if terminal["labels"]["hitty.owner"] != owner {
                            continue;
                        }
                        if let Some(address) = terminal["address"].as_str() {
                            let _ = core.call("terminal.kill", json!({"terminal": address}));
                            let _ = core.call("terminal.rm", json!({"terminal": address}));
                        }
                    }
                }
                Err(error) => eprintln!("hc cleanup unavailable for test profile {owner}: {error}"),
            }
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn size() -> WinSize {
    WinSize {
        cols: 120,
        rows: 30,
        cell_w: 8,
        cell_h: 16,
    }
}

fn fake_core() -> PathBuf {
    static FIXTURE: std::sync::OnceLock<(tempfile::TempDir, PathBuf)> = std::sync::OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            let dir = tempfile::TempDir::new().unwrap();
            let binary = dir.path().join(if cfg!(windows) { "hc.exe" } else { "hc" });
            compile_fixture("fake_hc.rs", &binary);
            (dir, binary)
        })
        .1
        .clone()
}

fn compile_fixture(source: &str, binary: &Path) {
    let mut command = Command::new("rustc");
    command
        .arg("--edition=2024")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(source),
        )
        .arg("-o")
        .arg(binary);
    hide_console(&mut command);
    let output = command.output().expect("compile the standalone fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
fn console_agent(dir: &Path) -> PathBuf {
    let binary = dir.join("claude.exe");
    compile_fixture("console_agent.rs", &binary);
    std::fs::copy(&binary, dir.join("test-editor.exe")).unwrap();
    binary
}

fn output_until(pane: &mut PaneSession, marker: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    loop {
        assert!(
            Instant::now() < deadline,
            "waiting for {marker}: {}",
            String::from_utf8_lossy(&output)
        );
        match pane.recv().unwrap_or_else(|e| {
            panic!(
                "waiting for {marker}: {e}; output: {}",
                String::from_utf8_lossy(&output)
            )
        }) {
            DaemonMsg::Output(bytes) | DaemonMsg::Snapshot(bytes) => output.extend(bytes),
            DaemonMsg::Exited { code } => {
                panic!("pane exited {code:?}: {}", String::from_utf8_lossy(&output))
            }
            _ => {}
        }
        if String::from_utf8_lossy(&output).contains(marker) {
            return String::from_utf8_lossy(&output).into_owned();
        }
    }
}

#[test]
fn closed_hc_transport_reports_failure_and_cleans_up_without_panicking() {
    let mut instance = Instance::new(fake_core(), true);
    let core = instance.core.as_mut().unwrap();
    core.child.kill().unwrap(); // only this test's fake hc connect process
    core.child.wait().unwrap();
    assert!(core.call("terminal.list", json!({})).is_err());
    drop(instance);
}

#[test]
fn missing_hc_falls_back_to_an_interactive_native_pane() {
    let mut instance = Instance::new(
        std::env::temp_dir().join("hitty-definitely-missing-hc.exe"),
        false,
    );
    instance.start();
    let mut pane = instance.spawn();
    pane.input(b"echo HITTY_NATIVE_FALLBACK\r").unwrap();
    output_until(&mut pane, "HITTY_NATIVE_FALLBACK");
    pane.kill().unwrap();
    instance.stop();
}

#[test]
fn incompatible_hc_falls_back_to_an_interactive_native_pane() {
    for mode in [
        "wrong-binary",
        "generation",
        "old-version",
        "missing-methods",
    ] {
        let mut instance = Instance::new(fake_core(), false);
        std::fs::write(instance.dir.path().join("fake-hc-mode"), mode).unwrap();
        instance.start();
        let mut pane = instance.spawn();
        pane.input(b"echo HITTY_COMPAT_FALLBACK\r").unwrap();
        output_until(&mut pane, "HITTY_COMPAT_FALLBACK");
        pane.kill().unwrap();
        instance.stop();
    }
}

#[test]
fn fake_hc_process_hosts_a_pane_without_an_installed_core() {
    let mut instance = Instance::new(fake_core(), false);
    instance.start();
    let mut pane = instance.spawn();
    output_until(&mut pane, "FAKE_HC_READY");
    pane.input(b"FAKE_HC_INPUT\r").unwrap();
    output_until(&mut pane, "FAKE_HC_INPUT");
    let mut resized = size();
    resized.cols = 99;
    resized.rows = 33;
    pane.resize(resized).unwrap();
    output_until(&mut pane, "FAKE_RESIZE:99x33");
    pane.input(b"exit\r").unwrap();
    loop {
        if let DaemonMsg::Exited { code } = pane.recv().unwrap() {
            assert_eq!(code, Some(7));
            break;
        }
    }
    instance.stop();
}

#[test]
fn real_hc_panes_survive_daemon_restart_and_close_only_their_own_terminals() {
    let Some(hc) = installed_core() else {
        eprintln!("hc not installed; skipping real engine lifecycle");
        return;
    };
    let mut instance = Instance::new(hc, true);
    instance.start();
    let mut first = instance.spawn();
    let mut second = instance.spawn();
    let first_id = first.pane_id();
    let second_id = second.pane_id();
    let terminals = instance.terminals();
    assert_eq!(terminals.len(), 2, "both panes must use hc, not a fallback");
    let terminal = terminals
        .iter()
        .find(|t| t["labels"]["hitty.pane"] == format!("%{first_id}"))
        .unwrap();
    let address = terminal["address"].as_str().unwrap().to_owned();
    let pid = terminal["pid"].clone();
    assert_eq!(terminal["keep"], false);
    assert!(terminal["parent"].is_null());
    let identity = format!("HC={address};HITTY={first_id}");
    let command = if cfg!(windows) {
        "set HITTY_TEST_SENTINEL=kept\recho HC=%HC_TERMINAL_ID%;HITTY=%HITTY_PANE%\r"
    } else {
        "HITTY_TEST_SENTINEL=kept; echo HC=$HC_TERMINAL_ID\\;HITTY=$HITTY_PANE\r"
    };
    first.input(command.as_bytes()).unwrap();
    output_until(&mut first, &identity);
    second.input(b"echo SECOND_PANE_READY\r").unwrap();
    output_until(&mut second, "SECOND_PANE_READY");
    let mut resized = size();
    resized.cols = 99;
    resized.rows = 33;
    first.resize(resized).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let t = instance
            .core
            .as_mut()
            .unwrap()
            .call("terminal.get", json!({"terminal": address}))
            .unwrap();
        if t["cols"] == 99 && t["rows"] == 33 {
            break;
        }
        assert!(Instant::now() < deadline, "resize did not reach hc");
        std::thread::sleep(Duration::from_millis(20));
    }
    first.detach().unwrap();
    second.detach().unwrap();
    instance.stop();
    let t = instance
        .core
        .as_mut()
        .unwrap()
        .call("terminal.get", json!({"terminal": address}))
        .unwrap();
    assert_eq!(t["state"], "live", "shutdown must detach");
    assert_eq!(t["pid"], pid);
    instance.start();
    assert!(
        instance
            .panes()
            .list()
            .unwrap()
            .iter()
            .any(|p| p.pane_id == first_id && p.alive)
    );
    let mut restored = instance.panes().attach(first_id, resized).unwrap();
    restored
        .set_recv_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    output_until(&mut restored, &identity); // hc replay contains the old screen
    restored
        .input(if cfg!(windows) {
            b"echo RESTORED=%HITTY_TEST_SENTINEL%\r"
        } else {
            b"echo RESTORED=$HITTY_TEST_SENTINEL\r"
        })
        .unwrap();
    output_until(&mut restored, "RESTORED=kept");
    restored.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if !instance.terminals().iter().any(|t| t["address"] == address) {
            break;
        }
        assert!(Instant::now() < deadline, "pane close left an hc terminal");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        instance
            .panes()
            .list()
            .unwrap()
            .iter()
            .any(|p| p.pane_id == second_id && p.alive)
    );
    instance.panes().kill(second_id).unwrap();
    // Kill is fire-and-forget. A Shutdown on another connection is not a
    // barrier for it and deliberately preserves any panes still registered.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !instance.terminals().is_empty() {
        assert!(
            Instant::now() < deadline,
            "second pane close left an hc terminal"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    instance.stop();
    assert!(instance.terminals().is_empty());
    instance.start();
    let mut natural = instance.spawn();
    natural
        .input(if cfg!(windows) {
            b"exit /b 7\r"
        } else {
            b"exit 7\r"
        })
        .unwrap();
    loop {
        if let DaemonMsg::Exited { code } = natural.recv().unwrap() {
            assert_eq!(code, Some(7));
            break;
        }
    }
    drop(natural);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !instance.terminals().is_empty() {
        assert!(
            Instant::now() < deadline,
            "natural exit left an hc terminal"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    instance.stop();
}

#[cfg(windows)]
#[test]
fn real_hc_preserves_negative_windows_exit_codes() {
    let Some(hc) = installed_core() else {
        eprintln!("hc not installed; skipping signed Windows exit-code probe");
        return;
    };
    let mut instance = Instance::new(hc, true);
    instance.start();
    let mut pane = instance.spawn();
    pane.input(b"exit /b -1\r").unwrap();
    loop {
        if let DaemonMsg::Exited { code } = pane.recv().unwrap() {
            assert_eq!(
                code,
                Some(-1),
                "a failing Windows status must not become success"
            );
            break;
        }
    }
}

#[cfg(windows)]
#[test]
fn real_hc_reports_external_editor_input_ownership() {
    let Some(hc) = installed_core() else {
        eprintln!("hc not installed; skipping agent input ownership probe");
        return;
    };
    let mut instance = Instance::new(hc, true);
    let agent = console_agent(instance.dir.path());
    instance.start();
    let mut pane = instance.spawn();
    pane.input(format!("\"{}\"\r", agent.display()).as_bytes())
        .unwrap();
    // Agent detection is throttled and runs on output. Keep the fixture
    // producing readiness output until the public protocol confirms detection.
    pane.set_recv_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "fixture agent was not detected");
        match pane.recv() {
            Ok(DaemonMsg::Agent(Some(hitty_core::core::cli_agent::CLIAgent::Claude))) => break,
            Ok(DaemonMsg::Exited { code }) => panic!("fixture exited before detection: {code:?}"),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                pane.input(b"ready\r").unwrap();
            }
            Err(error) => panic!("agent readiness: {error}"),
        }
    }
    pane.set_recv_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    pane.input(b"edit\r").unwrap();
    for expected in [true, false] {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                Instant::now() < deadline,
                "missing input ownership: {expected}"
            );
            if let DaemonMsg::AgentInputSuspended(suspended) = pane
                .recv()
                .unwrap_or_else(|error| panic!("input ownership {expected}: {error}"))
            {
                if suspended == expected {
                    break;
                }
            }
        }
        if expected {
            pane.input(b"return\r").unwrap();
        }
    }
    pane.input(b"exit\r").unwrap();
    pane.kill().unwrap();
}

#[cfg(windows)]
#[test]
fn real_hc_panes_exchange_messages_and_wait_for_children() {
    let Some(hc) = installed_core() else {
        eprintln!("hc not installed; skipping pane messaging");
        return;
    };
    let mut instance = Instance::new(hc, true);
    instance.start();
    let mut first = instance.spawn();
    let mut second = instance.spawn();
    let terminals = instance.terminals();
    let address = |id| {
        terminals
            .iter()
            .find(|t| t["labels"]["hitty.pane"] == format!("%{id}"))
            .unwrap()["address"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let a = address(first.pane_id());
    let b = address(second.pane_id());
    let script = instance.dir.path().join("peer.ps1");
    std::fs::write(&script, include_str!("fixtures/hc_peer.ps1")).unwrap();
    let command = |peer: &str, role: &str| {
        format!(
            "powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File \"{}\" \"{}\" {} {}\r",
            script.display(),
            instance.hc.display(),
            peer,
            role
        )
    };
    second.input(command(&a, "receiver").as_bytes()).unwrap();
    output_until(&mut second, "PEER_READY");
    first.input(command(&b, "sender").as_bytes()).unwrap();
    output_until(&mut second, "VERIFIED_PING_FROM_FIRST");
    output_until(&mut first, "VERIFIED_PONG_FROM_SECOND");

    let owner = instance.owner();
    let child = instance
        .core
        .as_mut()
        .unwrap()
        .call(
            "terminal.run",
            json!({
                "argv": ["cmd.exe", "/d", "/c", "ping -n 2 127.0.0.1 >nul & exit /b 9"],
                "parent": a, "no_notify": true, "keep": false,
                "labels": {"hitty.owner": owner}
            }),
        )
        .unwrap();
    assert_eq!(child["parent"], a);
    // Run the real CLI inside the pane: it must infer the parent from HC_TERMINAL_ID.
    let wait_script = instance.dir.path().join("wait.ps1");
    std::fs::write(&wait_script, r#"
param([string]$CoreBinary, [string]$ExpectedChild)
$result = & $CoreBinary wait --children all --timeout 10 --json | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or -not $result.ended) { exit 1 }
if (-not ($result.children | Where-Object { $_.terminal -eq $ExpectedChild -and $_.exit_code -eq 9 })) { exit 2 }
[Console]::WriteLine('VERIFIED_CHILD_WAIT')
"#).unwrap();
    first.input(format!(
        "powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File \"{}\" \"{}\" {}\r",
        wait_script.display(), instance.hc.display(), child["address"].as_str().unwrap()
    ).as_bytes()).unwrap();
    output_until(&mut first, "VERIFIED_CHILD_WAIT");
    // Instance drop cleans these test-owned parents and their child by label.
}

/// Real console semantics; physical IME composition still needs a GUI check.
#[cfg(windows)]
#[test]
fn real_hc_preserves_win32_keys_and_chinese_text() {
    let Some(hc) = installed_core() else {
        eprintln!("hc not installed; skipping Windows input probe");
        return;
    };
    let mut instance = Instance::new(hc, true);
    instance.start();
    let mut pane = instance.spawn();
    assert_eq!(instance.terminals().len(), 1);
    let script = instance.dir.path().join("input.ps1");
    std::fs::write(&script, r#"
[Console]::WriteLine('KEY_READY')
$key = [Console]::ReadKey($true)
[Console]::WriteLine(('KEY={0};MOD={1};CHAR={2}' -f [int]$key.Key, [int]$key.Modifiers, [int]$key.KeyChar))
[Console]::WriteLine('TEXT_READY')
$line = [Console]::ReadLine()
[Console]::WriteLine('TEXT=' + $line)
"#).unwrap();
    pane.input(
        format!(
            "powershell.exe -NoLogo -NoProfile -File \"{}\"\r",
            script.display()
        )
        .as_bytes(),
    )
    .unwrap();
    let ready = output_until(&mut pane, "KEY_READY");
    assert!(
        ready.contains("\x1b[?9001h"),
        "win32-input-mode must reach the viewer: {ready:?}"
    );
    // The exact encoding emitted by Hitty's existing Shift+Enter handler.
    pane.input(b"\x1b[13;28;13;1;16;1_\x1b[13;28;13;0;16;1_")
        .unwrap();
    let response = output_until(&mut pane, "TEXT_READY");
    assert!(response.contains("KEY=13;MOD=2;CHAR=13"), "{response:?}");
    pane.input("中文输入往返\r".as_bytes()).unwrap();
    output_until(&mut pane, "TEXT=中文输入往返");
}

/// Copy only Cmder's shell runtime so acceptance cannot rewrite user history.
#[cfg(windows)]
#[test]
fn real_hc_starts_cmder_with_isolated_history() {
    let Some(root) = std::env::var_os("HITTY_TEST_CMDER_ROOT").map(PathBuf::from) else {
        eprintln!("HITTY_TEST_CMDER_ROOT not set; skipping optional Cmder smoke");
        return;
    };
    let hc = installed_core().expect("hc is required for Cmder acceptance");
    let mut instance = Instance::new(hc, true);
    let copied = instance.dir.path().join("cmder");
    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &to.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }
    std::fs::create_dir_all(copied.join("vendor")).unwrap();
    for name in ["bin", "lib", "clink", "clink-completions"] {
        copy_tree(
            &root.join("vendor").join(name),
            &copied.join("vendor").join(name),
        );
    }
    for entry in std::fs::read_dir(root.join("vendor")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            std::fs::copy(entry.path(), copied.join("vendor").join(entry.file_name())).unwrap();
        }
    }
    std::fs::create_dir_all(copied.join("config")).unwrap();
    std::fs::create_dir_all(copied.join("bin")).unwrap();
    let mut profile = std::fs::read(copied.join("vendor/user_profile.cmd.default")).unwrap();
    profile.extend_from_slice(b"\r\necho CMDER_TEST_INITIALIZED\r\n");
    std::fs::write(copied.join("config/user_profile.cmd"), profile).unwrap();
    std::fs::write(instance.dir.path().join("config.json"), json!({
        "pane_engine": "herdr-core", "env": {"CMDER_ROOT": copied, "CMDER_USER_CONFIG": copied.join("config")}
    }).to_string()).unwrap();
    instance.start();
    let mut pane = instance.spawn_shell(ShellSpec {
        program: "cmd.exe".into(),
        args: vec![
            "/d".into(),
            "/k".into(),
            copied
                .join("vendor/init.bat")
                .to_string_lossy()
                .into_owned(),
        ],
        args_are_hitty_defaults: false,
    });
    output_until(&mut pane, "CMDER_TEST_INITIALIZED");
    assert_eq!(instance.terminals().len(), 1);
    pane.input(b"set /a 73000+421\r").unwrap();
    output_until(&mut pane, "73421");
    pane.input(b"\x1b[A\r").unwrap();
    output_until(&mut pane, "73421");
    pane.input(b"exit\r").unwrap();
    loop {
        if let DaemonMsg::Exited { .. } = pane.recv().unwrap() {
            break;
        }
    }
}

/// Engine transport timing, deliberately separate from correctness assertions.
#[cfg(windows)]
#[test]
#[ignore = "run explicitly on an idle Windows desktop for latency evidence"]
fn measure_native_and_herdr_key_echo_latency() {
    let hc = installed_core().expect("hc is required for the comparison");
    for engine in ["native", "herdr-core"] {
        let mut instance = Instance::new(hc.clone(), engine == "herdr-core");
        std::fs::write(
            instance.dir.path().join("config.json"),
            json!({"pane_engine": engine}).to_string(),
        )
        .unwrap();
        instance.start();
        let mut pane = instance.spawn();
        if engine == "herdr-core" {
            assert_eq!(instance.terminals().len(), 1);
        }
        pane.input(b"rem ").unwrap();
        output_until(&mut pane, "rem ");
        let mut samples = Vec::new();
        for index in 0..110 {
            let start = Instant::now();
            pane.input(b"~").unwrap();
            output_until(&mut pane, "~");
            if index >= 10 {
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{}",
            json!({"engine": engine, "samples": samples.len(),
            "median_ms": samples[50], "p95_ms": samples[94], "max_ms": samples[99]})
        );
        pane.kill().unwrap();
        if instance.core.is_some() {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !instance.terminals().is_empty() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        instance.stop();
    }
}

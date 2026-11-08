//! Exercise a remote pane through the production router and client APIs.
use hitty_core::client::{
    ControlClient, ControlHello, ControlRequest, DaemonMsg, PaneClient, RouteTarget, WinSize,
};
use hitty_core::daemon::install::{
    InstallConfirm, InstallDecision, InstallRequest, set_install_confirm,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct ConfirmFixtureInstall;
impl InstallConfirm for ConfirmFixtureInstall {
    fn confirm(&self, request: &InstallRequest) -> InstallDecision {
        assert!(
            request
                .remote_path
                .starts_with("/home/hittycheck/.local/share/hitty/bin/")
        );
        assert_eq!(
            request.sha256,
            std::env::var("HITTY_ACCEPTANCE_SERVER_SHA256").expect("expected fixture asset hash")
        );
        println!("SSH installation: {request:?}");
        InstallDecision::Approve
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        (2..=3).contains(&args.len()),
        "usage: bootstrap-remote-pane-smoke <route-json> [marker]"
    );
    let route: RouteTarget = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    match &route {
        RouteTarget::Wsl { distro } => assert!(distro.starts_with("Hitty-Acceptance-")),
        RouteTarget::Ssh(spec) => {
            assert_eq!(
                (&*spec.host, &*spec.user, spec.port),
                ("127.0.0.1", "hittycheck", 22476)
            );
            set_install_confirm(Arc::new(ConfirmFixtureInstall));
        }
        _ => panic!("only isolated WSL and loopback SSH fixtures are supported"),
    }
    let control = ControlClient::routed(
        route.clone(),
        &ControlHello::host_rpc("bootstrap-smoke", "acceptance"),
    )?;
    println!("Remote control: {:?}", control.hello());
    println!(
        "Remote tree: {:?}",
        control.request(ControlRequest::MachineGet)?
    );
    let Some(marker) = args.get(2) else {
        return Ok(());
    };
    assert!(marker.chars().all(|c| c.is_ascii_uppercase() || c == '_'));
    assert!(!marker.is_empty());
    let panes = PaneClient::routed(route);
    let list = panes.list()?;
    let alive: Vec<_> = list.iter().filter(|pane| pane.alive).collect();
    assert_eq!(
        alive.len(),
        1,
        "the acceptance fixture must have exactly one live pane"
    );
    let pane = alive[0].pane_id;
    let size = WinSize {
        cols: 100,
        rows: 30,
        cell_w: 8,
        cell_h: 16,
    };
    let mut session = panes.observe(pane, size)?;
    session.set_recv_timeout(Some(Duration::from_secs(15)))?;
    // Split the marker so echoed input cannot satisfy the output assertion.
    let (left, right) = marker.split_at(marker.len() / 2);
    panes.send_input(
        pane,
        format!("printf '%s%s\\n' '{left}' '{right}'; uname -sm\r").as_bytes(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        match session.recv()? {
            DaemonMsg::Output(bytes) | DaemonMsg::Snapshot(bytes) => output.extend(bytes),
            _ => {}
        }
        if String::from_utf8_lossy(&output).contains(marker) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&output);
    assert!(
        text.contains(marker),
        "remote command did not produce its marker: {text}"
    );
    println!("pane={pane}\n{text}");
    session.detach()?;
    Ok(())
}

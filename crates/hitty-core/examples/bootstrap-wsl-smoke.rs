//! Exercise the production WSL installer against a disposable acceptance distro.
use hitty_core::daemon::install::{
    InstallConfirm, InstallDecision, InstallRequest, Installer,
    wsl::{BundledServerBinary, WslRemoteOps, host_label},
};
use std::path::PathBuf;

struct ConfirmTestInstall;
impl InstallConfirm for ConfirmTestInstall {
    fn confirm(&self, request: &InstallRequest) -> InstallDecision {
        println!("Install request: {request:?}");
        InstallDecision::Approve
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: bootstrap-wsl-smoke <distro> <bundled-server-dir>"
    );
    let distro = &args[1];
    assert!(
        distro.starts_with("Hitty-Acceptance-"),
        "use a disposable acceptance distribution, never a personal one"
    );
    let source = BundledServerBinary::in_dirs(vec![PathBuf::from(&args[2])]);
    let ops = WslRemoteOps::new(distro);
    let install = Installer::with_source(&ops, &source, &ConfirmTestInstall, host_label(distro));
    let first = install.run().expect("fresh WSL deployment");
    println!("First connection: {first:?}");
    assert!(first.installed && first.confirmed && first.launched);
    assert!(first.mismatch.is_none());
    let second = install.run().expect("reuse the running WSL server");
    println!("Second connection: {second:?}");
    assert!(!second.installed && !second.confirmed && !second.launched);
    assert!(second.mismatch.is_none());
    println!("WSL_DEPLOYMENT_OK");
}

//! Keep native Windows Codex clients attributable to their Hitty pane.
//!
//! A shared app-server inherits only its first client's environment. Current
//! Codex uses its embedded server for a per-invocation environment override,
//! giving hooks and open-rollout probes the correct process owner.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use portable_pty::CommandBuilder;

const ORIGINAL: &str = "HITTY_CODEX_PROGRAM";
const SHELL: &str = "HITTY_CODEX_SHELL";
const BIN: &str = "hitty-agent-bin";
static LAUNCHER: Mutex<Option<PathBuf>> = Mutex::new(None);

fn pane_config(pane: u64) -> String {
    format!("shell_environment_policy.set.HITTY_PANE='{pane}'")
}

/// Entry point for the CLI's pane-local `codex.exe` copy, before clap parsing.
pub fn run_if_launcher() {
    if !std::env::current_exe()
        .ok()
        .is_some_and(|exe| is_launcher_path(&exe))
    {
        return;
    }
    let result = (|| {
        let original = std::env::var_os(ORIGINAL)
            .ok_or_else(|| std::io::Error::other("Codex launcher has no original executable"))?;
        let pane = std::env::var("HITTY_PANE")
            .ok()
            .and_then(|id| id.parse::<u64>().ok())
            .ok_or_else(|| std::io::Error::other("Codex launcher has no pane ID"))?;
        let shell = std::env::var_os(SHELL);
        forwarded_command(
            original,
            shell.as_deref(),
            pane,
            std::env::args_os().skip(1),
        )
        .status()
    })();
    match result {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("hitty: could not start Codex: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn is_launcher_path(exe: &Path) -> bool {
    exe.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("codex.exe"))
        && exe
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == BIN)
}

pub(super) fn is_launcher(pid: u32) -> bool {
    super::winproc::image_path(pid).is_some_and(|path| is_launcher_path(&path))
}

fn forwarded_command(
    original: impl AsRef<std::ffi::OsStr>,
    shell: Option<&std::ffi::OsStr>,
    pane: u64,
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Command {
    let mut command = Command::new(original);
    if Path::new(command.get_program())
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
    {
        let script = command.get_program().to_os_string();
        command = Command::new(shell.unwrap_or_else(|| std::ffi::OsStr::new("powershell.exe")));
        command.args(["-NoLogo", "-NoProfile", "-File"]).arg(script);
    }
    command.args(["-c", &pane_config(pane)]).args(args);
    command
}

pub(super) fn prepare(
    command: &mut CommandBuilder,
    shell: &str,
    pane: u64,
    integration: &mut Option<PathBuf>,
) -> std::io::Result<()> {
    if !crate::core::config::Config::load().restore_agent_sessions {
        return Ok(());
    }
    let shell_program = shell;
    let shell = Path::new(shell)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if shell.eq_ignore_ascii_case("codex") {
        command
            .get_argv_mut()
            .splice(1..1, ["-c".into(), pane_config(pane).into()]);
        return Ok(());
    }
    if !["cmd", "powershell", "pwsh", "nu"]
        .iter()
        .any(|name| shell.eq_ignore_ascii_case(name))
    {
        return Ok(());
    }
    let Some(path) = command.get_env("PATH").map(|p| p.to_os_string()) else {
        return Ok(());
    };
    let names: &[&str] =
        if shell.eq_ignore_ascii_case("powershell") || shell.eq_ignore_ascii_case("pwsh") {
            &["codex.exe", "codex.ps1", "codex.cmd", "codex.bat"]
        } else {
            &["codex.exe", "codex.cmd", "codex.bat"]
        };
    let Some(original) = std::env::split_paths(&path)
        .filter(|dir| dir.file_name().is_none_or(|name| name != BIN))
        .find_map(|dir| {
            names
                .iter()
                .map(|name| dir.join(name))
                .find(|candidate| candidate.is_file())
        })
    else {
        return Ok(());
    };
    let cli = std::env::current_exe()?.with_file_name("hitty.exe");
    if !cli.is_file() {
        return Ok(()); // The headless server can be distributed without the GUI CLI.
    }
    let directory = integration.get_or_insert_with(|| {
        std::env::temp_dir().join(format!("hitty-codex-{}-{pane}", std::process::id()))
    });
    let bin = directory.join(BIN);
    std::fs::create_dir_all(&bin)?;
    let launcher = bin.join("codex.exe");
    let mut source = LAUNCHER.lock().unwrap();
    // Copy away from the installed image so a running launcher never locks an
    // installer or Cargo's output. Other panes share only this temporary copy.
    if !source
        .as_ref()
        .is_some_and(|source| std::fs::hard_link(source, &launcher).is_ok())
    {
        std::fs::copy(cli, &launcher)?;
        *source = Some(launcher);
    }
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path)))
        .map_err(std::io::Error::other)?;
    command.env("PATH", path);
    command.env(ORIGINAL, original);
    command.env(SHELL, shell_program);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_forwarding_keeps_literal_arguments() {
        let args = [
            "resume",
            "exact-id",
            "--model",
            "a model",
            "explain %TEMP% & $x; 'quoted' 中文",
        ]
        .map(std::ffi::OsString::from);
        let command = forwarded_command("original codex.exe", None, 42, args.clone());
        let expected = [std::ffi::OsString::from("-c"), pane_config(42).into()]
            .into_iter()
            .chain(args)
            .collect::<Vec<_>>();
        assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn powershell_install_keeps_its_script_and_multiline_arguments() {
        let command = forwarded_command(
            "original codex.ps1",
            None,
            42,
            ["line one\nline two".into()],
        );
        assert_eq!(command.get_program(), "powershell.exe");
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(
            &args[..4],
            ["-NoLogo", "-NoProfile", "-File", "original codex.ps1"]
        );
        assert_eq!(args.last().unwrap(), &"line one\nline two");
    }
}

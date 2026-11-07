//! Read-only Git queries over a managed pane's existing SSH connection.
use std::time::Duration;

use russh::ChannelMsg;

use crate::daemon::install::shell_quote;
use crate::host::Output;

use super::{SshConnection, SshManager};

pub const QUERY_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_OUTPUT: usize = 8 * 1024 * 1024;

/// Run on the existing SSH runtime from a daemon request thread.
pub fn run(conn: &SshConnection, cwd: &str, args: &[String]) -> Result<Output, String> {
    SshManager::global()
        .handle()
        .block_on(query(conn, cwd, args, QUERY_TIMEOUT))
}

/// Query one established connection. The deadline includes channel setup and
/// command acknowledgement; dropping the exchange also closes its channel.
pub async fn query(
    conn: &SshConnection,
    cwd: &str,
    args: &[String],
    timeout: Duration,
) -> Result<Output, String> {
    let command = command(cwd, args)?;
    if !conn.is_alive() {
        return Err("Native SSH connection is closed".into());
    }
    tokio::time::timeout(timeout.min(QUERY_TIMEOUT), async {
        let mut channel = conn
            .open_command_channel()
            .await
            .map_err(|e| format!("could not open Git channel: {e}"))?;
        channel
            .exec(true, command)
            .await
            .map_err(|e| format!("could not start remote Git: {e}"))?;
        let _ = channel.eof().await;
        let mut output = Output {
            status: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }
                    if output
                        .stdout
                        .len()
                        .saturating_add(output.stderr.len())
                        .saturating_add(data.len())
                        > MAX_OUTPUT =>
                {
                    return Err("remote Git output exceeded 8 MiB".into());
                }
                ChannelMsg::Data { data } => output.stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => output.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => {
                    output.status = i32::try_from(exit_status).ok()
                }
                _ => {}
            }
        }
        Ok(output)
    })
    .await
    .map_err(|_| "remote Git query timed out".to_string())?
}

fn command(cwd: &str, args: &[String]) -> Result<String, String> {
    if cwd.is_empty()
        || !cwd.starts_with('/')
        || cwd.contains('\0')
        || args.iter().any(|arg| arg.contains('\0'))
        || cwd
            .len()
            .saturating_add(args.iter().map(String::len).sum::<usize>())
            > 256 * 1024
        || args.len() > 512
    {
        return Err("invalid remote Git directory or arguments".into());
    }
    // The diff parser already requests this one fixed configuration value.
    // No caller-controlled configuration, aliases or additional Git verbs.
    let args = if args.first().map(String::as_str) == Some("-c")
        && args.get(1).map(String::as_str) == Some("core.quotePath=false")
    {
        &args[2..]
    } else {
        args
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let allowed = match args.as_slice() {
        [
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ]
        | ["rev-parse", "--show-toplevel"]
        | ["rev-parse", "--short", "HEAD"]
        | ["symbolic-ref", "--quiet", "--short", "HEAD"]
        | ["ls-files", "--others", "--exclude-standard", "--full-name"] => true,
        ["show", "--no-textconv", object] => !object.is_empty() && !object.starts_with('-'),
        ["diff", rest @ ..] => diff_arguments(rest, false),
        ["log", "-p", "-1", "--format=", "--first-parent", rest @ ..] => diff_arguments(rest, true),
        _ => false,
    };
    if !allowed {
        return Err("unsupported read-only Native SSH Git query".into());
    }
    let mut argv = vec![
        "git".to_string(),
        "--no-pager".into(),
        "--no-optional-locks".into(),
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        "core.quotePath=false".into(),
        "-C".into(),
        cwd.into(),
    ];
    // Never invoke external diff/text conversion, including the lightweight
    // numstat probe which does not supply these switches itself.
    for (index, arg) in args.iter().enumerate() {
        argv.push((*arg).to_string());
        if index == 0 && matches!(*arg, "diff" | "log" | "show") {
            argv.extend(["--no-ext-diff".into(), "--no-textconv".into()]);
        }
    }
    let argv = argv
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS; GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 LC_ALL=C exec {argv}"
    );
    Ok(format!("sh -c {}", shell_quote(&script)))
}

fn diff_arguments(args: &[&str], require_revision: bool) -> bool {
    let mut revisions = 0;
    for arg in args {
        if *arg == "--" {
            return !require_revision || revisions == 1;
        }
        if matches!(
            *arg,
            "--cached"
                | "--numstat"
                | "--no-color"
                | "--no-ext-diff"
                | "--no-textconv"
                | "-M"
                | "-w"
        ) {
            continue;
        }
        if arg
            .strip_prefix("-U")
            .is_some_and(|n| n.parse::<u32>().is_ok())
        {
            continue;
        }
        if arg.is_empty() || arg.starts_with('-') || arg.chars().any(char::is_control) {
            return false;
        }
        revisions += 1;
        if revisions > 1 {
            return false;
        }
    }
    !require_revision || revisions == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::ssh::test_support::{Exec, FakeSshd};

    #[test]
    fn a_remote_git_query_preserves_output_bytes_stderr_and_failure_status() {
        super::super::SshManager::global().handle().block_on(async {
            let server = FakeSshd::connect(
                Exec::Reply {
                    stdout: b"branch\n\xff",
                    stderr: b"fatal: no HEAD\n",
                    status: 128,
                },
                Some(1),
            )
            .await;
            let result = query(
                &server.conn,
                "/work/repo",
                &["rev-parse".into(), "--short".into(), "HEAD".into()],
                Duration::from_secs(1),
            )
            .await
            .unwrap();
            assert_eq!(
                result,
                Output {
                    stdout: b"branch\n\xff".to_vec(),
                    stderr: b"fatal: no HEAD\n".to_vec(),
                    status: Some(128)
                }
            );
        });
    }

    #[test]
    fn a_remote_git_query_refuses_mutations_and_unsafe_options() {
        SshManager::global().handle().block_on(async {
            let server = FakeSshd::connect(Exec::Exits, None).await;
            for args in [
                vec!["reset", "--hard"],
                vec!["-c", "alias.peek=!touch changed", "peek"],
                vec!["diff", "--output=changed"],
                vec!["diff", "--ext-diff"],
                vec!["diff", "--textconv"],
                vec!["diff", "--no-index", "/one", "/two"],
                vec!["show", "--no-textconv", "--output=changed"],
            ] {
                let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
                assert!(
                    query(&server.conn, "/repo", &args, Duration::from_secs(1))
                        .await
                        .is_err()
                );
            }
            assert_eq!(
                server.opened(),
                0,
                "rejected queries never reach the SSH server"
            );
        });
    }

    #[test]
    fn a_timed_out_git_query_releases_the_servers_only_session() {
        SshManager::global().handle().block_on(async {
            let server = FakeSshd::connect(Exec::Hangs, Some(1)).await;
            for n in 1..=2 {
                let result = query(
                    &server.conn,
                    "/repo",
                    &["rev-parse".into(), "--show-toplevel".into()],
                    Duration::from_millis(100),
                )
                .await;
                assert!(result.unwrap_err().contains("timed out"));
                server.wait_for_closed(n).await;
            }
            assert_eq!(
                server.refused(),
                0,
                "the next query can use the released session"
            );
        });
    }

    #[test]
    fn an_oversized_git_reply_is_rejected_instead_of_truncated() {
        static FLOOD: [u8; MAX_OUTPUT + 1] = [b'x'; MAX_OUTPUT + 1];
        SshManager::global().handle().block_on(async {
            let server = FakeSshd::connect(
                Exec::Reply {
                    stdout: &FLOOD,
                    stderr: b"",
                    status: 0,
                },
                Some(1),
            )
            .await;
            let result = query(
                &server.conn,
                "/repo",
                &["rev-parse".into(), "--show-toplevel".into()],
                Duration::from_secs(5),
            )
            .await;
            assert!(result.unwrap_err().contains("exceeded 8 MiB"));
        });
    }

    #[test]
    fn a_remote_git_query_treats_shell_metacharacters_in_paths_as_literals() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo's $literal; space");
        std::fs::create_dir(&root).unwrap();
        let file = "file's $literal; test.txt";
        let git = |args: &[&str]| {
            let mut command = std::process::Command::new("git");
            command.arg("-C").arg(&root).args(args);
            let result = crate::core::proc::hide_console(&mut command)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        };
        git(&["init", "-b", "main"]);
        std::fs::write(root.join(file), "before\n").unwrap();
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "base",
        ]);
        std::fs::write(root.join(file), "after\nadded\n").unwrap();
        let remote = root.to_string_lossy().replace('\\', "/");
        #[cfg(windows)]
        // A slash-rooted absolute path understood by native Windows Git;
        // do not depend on MSYS argument conversion in the test shell.
        let remote = format!("//?/{remote}");
        SshManager::global().handle().block_on(async {
            let server = FakeSshd::connect(Exec::LocalShell, None).await;
            let args = ["diff", "--numstat", "HEAD", "--", file].map(str::to_owned);
            let result = query(&server.conn, &remote, &args, Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(
                result.status,
                Some(0),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(result.stdout, format!("2\t1\t{file}\n").as_bytes());

            // The same request shape used by the viewer must survive the real
            // SSH exchange, including full context and a literal pathspec.
            use crate::core::git::diff::{DiffParser, DiffRequest, DiffSource};
            let paths = [format!(":(literal){file}")];
            let args = DiffRequest {
                source: DiffSource::Head,
                paths: &paths,
                context: 100,
                ..Default::default()
            }
            .args();
            let result = query(&server.conn, &remote, &args, Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(
                result.status,
                Some(0),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let mut parser = DiffParser::default();
            for line in String::from_utf8(result.stdout).unwrap().lines() {
                parser.push_line(line);
            }
            let files = parser.finish();
            assert_eq!(files.len(), 1);
            assert_eq!(files[0].path, file);
            assert_eq!((files[0].added, files[0].removed), (2, 1));
            assert_eq!(files[0].hunks[0].lines[0].text, "before");
            assert_eq!(files[0].hunks[0].lines[1].text, "after");
            assert_eq!(files[0].hunks[0].lines[2].text, "added");
        });
    }
}

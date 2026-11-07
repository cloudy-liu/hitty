//! Recoverable file deletion over the owning Native SSH connection.

use super::{SshConnection, SshManager};
use russh::ChannelMsg;
use std::time::Duration;

// Fixed command; paths travel only as JSON on stdin. gio owns native trash
// placement, XDG configuration, mount handling, and recovery metadata.
const COMMAND: &str = "python3 -c 'import json,os,subprocess,sys; p=json.load(sys.stdin); assert sys.platform.startswith(\"linux\") and isinstance(p,str) and p.startswith(\"/\") and os.path.normpath(p)!=\"/\" and \"\\x00\" not in p; r=subprocess.run([\"gio\",\"trash\",\"--\",p],stdout=subprocess.PIPE,stderr=subprocess.PIPE); sys.stdout.buffer.write(r.stdout); sys.stderr.buffer.write(r.stderr); sys.exit(r.returncode)'";

pub fn run(conn: &SshConnection, path: &str) -> Result<(), String> {
    SshManager::global().handle().block_on(query(conn, path))
}

pub async fn query(conn: &SshConnection, path: &str) -> Result<(), String> {
    if !path.starts_with('/')
        || path.trim_matches('/').is_empty()
        || path.split('/').any(|part| part == "." || part == "..")
        || path.contains('\0')
        || path.len() > 32 * 1024
    {
        return Err("invalid remote trash target".into());
    }
    if !conn.is_alive() {
        return Err("Native SSH connection is closed".into());
    }
    let input = serde_json::to_vec(path).map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut channel = conn
            .open_command_channel()
            .await
            .map_err(|e| e.to_string())?;
        channel
            .exec(true, COMMAND)
            .await
            .map_err(|e| e.to_string())?;
        channel
            .data(input.as_slice())
            .await
            .map_err(|e| format!("remote trash outcome unknown: {e}"))?;
        channel
            .eof()
            .await
            .map_err(|e| format!("remote trash outcome unknown: {e}"))?;
        let mut stderr = Vec::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::ExtendedData { data, .. } => {
                    if stderr.len() + data.len() > 64 * 1024 {
                        return Err("remote trash outcome unknown: error output limit".into());
                    }
                    stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
        match status {
            Some(0) => Ok(()),
            Some(_) => Err(format!(
                "remote trash failed: {}",
                String::from_utf8_lossy(&stderr).trim()
            )),
            None => Err("remote trash outcome unknown: connection closed without a result".into()),
        }
    })
    .await
    .map_err(|_| "remote trash outcome unknown: timed out; refresh before retrying".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::ssh::test_support::{Exec, FakeSshd};

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires TTY7_SSH_TEST_PYTHON with asyncssh and Linux gio"]
    fn native_ssh_trash_restores_file_folder_and_preserves_symlink_target() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            use crate::daemon::ssh::{
                ConnectionKey, broker::PromptBroker, forward::RemoteForwardTable,
                handler::ClientHandler,
            };
            use std::sync::Arc;
            use tokio::io::{AsyncBufReadExt, BufReader};
            let python =
                std::env::var("TTY7_SSH_TEST_PYTHON").expect("test Python fixture interpreter");
            let mut child = tokio::process::Command::new(python)
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/native_ssh_trash.py"
                ))
                .stdout(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let line = BufReader::new(child.stdout.take().unwrap())
                .lines()
                .next_line()
                .await
                .unwrap()
                .unwrap();
            let fixture: serde_json::Value = serde_json::from_str(&line).unwrap();
            let port = fixture["port"].as_u64().unwrap() as u16;
            let home = std::path::PathBuf::from(fixture["home"].as_str().unwrap());
            let xdg = std::path::PathBuf::from(fixture["xdg"].as_str().unwrap());
            let mut spec = crate::daemon::ssh::test_support::base_spec();
            spec.host = "127.0.0.1".into();
            spec.port = port;
            let forwards = RemoteForwardTable::default();
            let handler = ClientHandler {
                host: spec.host.clone(),
                port,
                verify_host_keys: false,
                skip_banner: true,
                broker: PromptBroker::new(Box::new(|_| true)),
                remote_forwards: forwards.clone(),
            };
            let mut handle = russh::client::connect(
                Arc::new(russh::client::Config::default()),
                ("127.0.0.1", port),
                handler,
            )
            .await
            .unwrap();
            let key = russh::keys::load_secret_key(fixture["key"].as_str().unwrap(), None).unwrap();
            let auth = handle
                .authenticate_publickey(
                    "fixture",
                    russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None),
                )
                .await
                .unwrap();
            assert!(auth.success());
            let conn = SshConnection::new(handle, ConnectionKey::from_spec(&spec), forwards);
            let file = home.join("file; quote'.txt");
            let folder = home.join("folder");
            let link = home.join("link");
            std::fs::write(&file, "file contents").unwrap();
            std::fs::create_dir(&folder).unwrap();
            std::fs::write(folder.join("child.txt"), "folder contents").unwrap();
            std::os::unix::fs::symlink(&folder, &link).unwrap();
            query(&conn, link.to_str().unwrap()).await.unwrap();
            assert_eq!(
                std::fs::read_to_string(folder.join("child.txt")).unwrap(),
                "folder contents"
            );
            for path in [&file, &folder] {
                query(&conn, path.to_str().unwrap()).await.unwrap();
                assert!(!path.exists());
            }
            let trash = xdg.join("Trash");
            for path in [&file, &folder] {
                let name = path.file_name().unwrap();
                assert!(
                    std::fs::read_to_string(
                        trash
                            .join("info")
                            .join(format!("{}.trashinfo", name.to_string_lossy()))
                    )
                    .unwrap()
                    .contains("Path=")
                );
                std::fs::rename(trash.join("files").join(name), path).unwrap();
            }
            assert_eq!(std::fs::read_to_string(file).unwrap(), "file contents");
            assert_eq!(
                std::fs::read_to_string(folder.join("child.txt")).unwrap(),
                "folder contents"
            );
            assert!(
                query(&conn, home.join("missing").to_str().unwrap())
                    .await
                    .is_err()
            );
            child.kill().await.unwrap();
            child.wait().await.unwrap();
        });
    }

    #[test]
    fn unavailable_remote_trash_is_an_error() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let server = FakeSshd::connect(
                Exec::Reply {
                    stdout: b"",
                    stderr: b"gio is unavailable",
                    status: 127,
                },
                None,
            )
            .await;
            let result = query(&server.conn, "/tmp/keep.txt").await;
            assert!(result.unwrap_err().contains("gio is unavailable"));
        });
    }
}

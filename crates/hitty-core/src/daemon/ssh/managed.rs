use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;

use super::ChannelCmd;
use crate::daemon::protocol::{ClientMsg, DaemonMsg, MAX_FRAME, WinSize};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) enum DriveEnd {
    KillRequested(Result<()>),
    Detached,
    Exited,
    Disconnected(anyhow::Error),
}

enum LocalStop {
    Kill,
    Detach,
}

pub(super) async fn spawn(
    link: &mut (impl AsyncRead + AsyncWrite + Unpin),
    cwd: Option<PathBuf>,
    size: WinSize,
) -> Result<u64> {
    let request = ClientMsg::Spawn {
        cwd,
        size,
        shell: None,
        owner: None,
        workspace: None,
        restore: None,
    };
    match request_reply(link, request).await? {
        DaemonMsg::Spawned { pane_id } => Ok(pane_id),
        DaemonMsg::Error(reason) => bail!("{reason}"),
        _ => bail!("unexpected reply to managed SSH spawn"),
    }
}

pub(super) async fn attach(
    link: &mut (impl AsyncRead + AsyncWrite + Unpin),
    pane_id: u64,
    size: WinSize,
) -> Result<DaemonMsg> {
    match request_reply(link, ClientMsg::Attach { pane_id, size }).await? {
        first @ (DaemonMsg::Size(_) | DaemonMsg::Snapshot(_)) => Ok(first),
        DaemonMsg::Error(reason) if reason == format!("no such pane {pane_id}") => {
            Err(io::Error::new(io::ErrorKind::NotFound, reason).into())
        }
        DaemonMsg::Error(reason) => bail!("{reason}"),
        _ => bail!("unexpected reply to managed SSH attach"),
    }
}

async fn request_reply(
    link: &mut (impl AsyncRead + AsyncWrite + Unpin),
    msg: ClientMsg,
) -> Result<DaemonMsg> {
    send(link, msg).await?;
    timeout(REQUEST_TIMEOUT, read_message(link))
        .await
        .context("timed out waiting for the remote pane")?
        .context("reading reply from the remote pane")
}

pub(super) async fn send(link: &mut (impl AsyncWrite + Unpin), msg: ClientMsg) -> Result<()> {
    let mut frame = Vec::new();
    msg.encode(&mut frame)?;
    timeout(REQUEST_TIMEOUT, async {
        link.write_all(&frame).await?;
        link.flush().await
    })
    .await
    .context("timed out writing to the remote pane")?
    .context("writing to the remote pane")
}

async fn read_message(link: &mut (impl AsyncRead + Unpin)) -> io::Result<DaemonMsg> {
    let mut header = [0; 5];
    link.read_exact(&mut header).await?;
    let len = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame payload exceeds MAX_FRAME",
        ));
    }
    let mut payload = vec![0; len];
    link.read_exact(&mut payload).await?;
    DaemonMsg::from_frame(header[4], payload)
}

/// Drive an attached, authenticated pane bridge using the daemon protocol.
pub(super) async fn drive(
    link: impl AsyncRead + AsyncWrite + Unpin,
    pane_id: u64,
    cmd_rx: &mut UnboundedReceiver<ChannelCmd>,
    size: &mut WinSize,
    handle_frame: impl FnMut(DaemonMsg),
) -> DriveEnd {
    let (mut reader, mut writer) = tokio::io::split(link);
    let mut stop = None;
    // Keep one read future alive for the whole attachment. Completing an
    // input write leaves any partly read frame in that future.
    let end = tokio::select! {
        end = receive_frames(&mut reader, handle_frame) => end,
        end = send_commands(&mut writer, pane_id, cmd_rx, size, &mut stop) => end,
    };
    // A disconnect can race an in-flight close write after the command has
    // left the queue. Preserve the caller's intent for reconnect handling.
    match (stop, end) {
        (Some(LocalStop::Kill), DriveEnd::Disconnected(error)) => {
            DriveEnd::KillRequested(Err(error))
        }
        (Some(LocalStop::Detach), DriveEnd::Disconnected(_)) => DriveEnd::Detached,
        (_, end) => end,
    }
}

async fn receive_frames(
    link: &mut (impl AsyncRead + Unpin),
    mut handle_frame: impl FnMut(DaemonMsg),
) -> DriveEnd {
    loop {
        match read_message(link).await {
            Ok(exited @ DaemonMsg::Exited { .. }) => {
                handle_frame(exited);
                return DriveEnd::Exited;
            }
            Ok(DaemonMsg::Error(reason)) => {
                return DriveEnd::Disconnected(anyhow::anyhow!("{reason}"));
            }
            Ok(frame) => handle_frame(frame),
            Err(error) => return DriveEnd::Disconnected(error.into()),
        }
    }
}

async fn send_commands(
    link: &mut (impl AsyncWrite + Unpin),
    pane_id: u64,
    cmd_rx: &mut UnboundedReceiver<ChannelCmd>,
    size: &mut WinSize,
    stop: &mut Option<LocalStop>,
) -> DriveEnd {
    loop {
        let msg = match cmd_rx.recv().await {
            Some(ChannelCmd::Data(bytes)) => ClientMsg::Input(bytes),
            Some(ChannelCmd::PassiveInput(bytes)) => ClientMsg::PassiveInput(bytes),
            Some(ChannelCmd::Resize(new_size)) => {
                *size = new_size;
                ClientMsg::Resize(new_size)
            }
            Some(ChannelCmd::Close) => {
                *stop = Some(LocalStop::Kill);
                return DriveEnd::KillRequested(send(link, ClientMsg::Kill { pane_id }).await);
            }
            Some(ChannelCmd::Detach) | None => {
                *stop = Some(LocalStop::Detach);
                let _ = send(link, ClientMsg::Detach).await;
                return DriveEnd::Detached;
            }
        };
        if let Err(error) = send(link, msg).await {
            return DriveEnd::Disconnected(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cli_agent::CLIAgent;
    use tokio::io::DuplexStream;
    use tokio::sync::mpsc;

    fn encode_reply(msg: DaemonMsg) -> Vec<u8> {
        let mut bytes = Vec::new();
        msg.encode(&mut bytes).unwrap();
        bytes
    }

    async fn expect_command(peer: &mut DuplexStream, msg: ClientMsg) {
        let mut expected = Vec::new();
        msg.encode(&mut expected).unwrap();
        let mut wire = vec![0; expected.len()];
        peer.read_exact(&mut wire).await.unwrap();
        assert_eq!(wire, expected);
    }

    #[tokio::test]
    async fn managed_stream_keeps_identity_and_output_when_input_arrives_mid_frame() {
        let mut size = WinSize {
            cols: 80,
            rows: 24,
            cell_w: 8,
            cell_h: 16,
        };
        let initial_size = size;
        let resized = WinSize { cols: 100, ..size };
        let (mut link, mut peer) = tokio::io::duplex(4);
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let agent = DaemonMsg::Agent(Some(CLIAgent::Codex));
        let output = DaemonMsg::Output(b"codex ready\r\n".to_vec());
        let exited = DaemonMsg::Exited { code: Some(7) };
        let client = async {
            let pane_id = spawn(&mut link, Some(PathBuf::from("/work")), initial_size)
                .await
                .unwrap();
            assert_eq!(pane_id, 42);
            let mut observed = Vec::new();
            let end = drive(link, pane_id, &mut cmd_rx, &mut size, |msg| {
                observed.push(msg);
            })
            .await;
            assert!(matches!(end, DriveEnd::Exited));
            assert_eq!(observed, [agent.clone(), output.clone(), exited.clone()]);
            assert_eq!(size, resized);
        };
        let server = async {
            expect_command(
                &mut peer,
                ClientMsg::Spawn {
                    cwd: Some(PathBuf::from("/work")),
                    size: initial_size,
                    shell: None,
                    owner: None,
                    workspace: None,
                    restore: None,
                },
            )
            .await;
            peer.write_all(&encode_reply(DaemonMsg::Spawned { pane_id: 42 }))
                .await
                .unwrap();
            let frame = encode_reply(agent.clone());
            // The four-byte channel forces part of the header to be read
            // before the peer can finish this write and queue input.
            peer.write_all(&frame[..5]).await.unwrap();
            cmd_tx.send(ChannelCmd::Data(b"hello".to_vec())).unwrap();
            cmd_tx
                .send(ChannelCmd::PassiveInput(b"\x1b[A".to_vec()))
                .unwrap();
            cmd_tx.send(ChannelCmd::Resize(resized)).unwrap();
            expect_command(&mut peer, ClientMsg::Input(b"hello".to_vec())).await;
            expect_command(&mut peer, ClientMsg::PassiveInput(b"\x1b[A".to_vec())).await;
            expect_command(&mut peer, ClientMsg::Resize(resized)).await;
            peer.write_all(&frame[5..]).await.unwrap();
            peer.write_all(&encode_reply(output.clone())).await.unwrap();
            peer.write_all(&encode_reply(exited.clone())).await.unwrap();
        };
        timeout(Duration::from_secs(2), async {
            tokio::join!(client, server);
        })
        .await
        .unwrap();

        let (mut link, mut peer) = tokio::io::duplex(64);
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let client = async {
            let first = attach(&mut link, 42, size).await.unwrap();
            assert_eq!(first, DaemonMsg::Size(initial_size));
            cmd_tx.send(ChannelCmd::Close).unwrap();
            let end = drive(link, 42, &mut cmd_rx, &mut size, |_| {}).await;
            assert!(matches!(end, DriveEnd::KillRequested(Ok(()))));
        };
        let server = async {
            expect_command(
                &mut peer,
                ClientMsg::Attach {
                    pane_id: 42,
                    size: resized,
                },
            )
            .await;
            peer.write_all(&encode_reply(DaemonMsg::Size(initial_size)))
                .await
                .unwrap();
            expect_command(&mut peer, ClientMsg::Kill { pane_id: 42 }).await;
        };
        timeout(Duration::from_secs(2), async {
            tokio::join!(client, server);
        })
        .await
        .unwrap();
    }
}

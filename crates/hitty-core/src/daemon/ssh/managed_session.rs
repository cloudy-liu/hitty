use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::managed::{self, DriveEnd};
use super::{ChannelCmd, PromptBroker, SharedConnection, SshConnection, SshManager};
use crate::daemon::protocol::{
    ClientMsg, DaemonMsg, DaemonVersion, ManagedSshPane, NativeSshSpec, SshPhase, WinSize,
};
use crate::daemon::remote_link::RemoteLink;

pub(crate) enum ManagedEvent {
    Bound(ManagedSshPane),
    Replay(WinSize),
    Frame(DaemonMsg),
}

/// An SSH PTY is still needed for features whose environment belongs to the
/// SSH login rather than to the server's already-running daemon.
pub(super) fn can_host(spec: &NativeSshSpec) -> bool {
    spec.shell_integration && !spec.agent_forward && !spec.x11 && spec.term == "xterm-256color"
}

pub(super) fn supports(version: &DaemonVersion) -> bool {
    use crate::daemon::protocol::{FEATURE_PASSIVE_INPUT, FEATURE_RESIZE_ECHO};
    !version.instance.is_empty()
        && [FEATURE_PASSIVE_INPUT, FEATURE_RESIZE_ECHO]
            .iter()
            .all(|feature| version.features.iter().any(|peer| peer == feature))
}

pub(super) struct Session<'a, F> {
    pub manager: &'a SshManager,
    pub spec: &'a NativeSshSpec,
    pub broker: &'a Arc<PromptBroker>,
    pub conn_slot: &'a SharedConnection,
    pub cmd_rx: mpsc::UnboundedReceiver<ChannelCmd>,
    pub size: WinSize,
    pub publish: F,
}

impl<F: FnMut(ManagedEvent) + Send> Session<'_, F> {
    pub async fn run(
        mut self,
        mut conn: Arc<SshConnection>,
        mut link: RemoteLink,
        version: DaemonVersion,
        cwd: Option<&str>,
    ) -> anyhow::Result<()> {
        let id = if let Some(saved) = &self.spec.managed {
            let frame = attach(self.manager, &conn, &mut link, &version, saved, self.size).await?;
            (self.publish)(ManagedEvent::Replay(self.size));
            (self.publish)(ManagedEvent::Frame(frame));
            managed::send(&mut link, ClientMsg::Resize(self.size)).await?;
            saved.clone()
        } else {
            let pane_id = managed::spawn(&mut link, cwd.map(PathBuf::from), self.size).await?;
            let id = ManagedSshPane {
                pane_id,
                instance: version.instance,
            };
            (self.publish)(ManagedEvent::Bound(id.clone()));
            for line in &self.spec.login_script {
                let input = format!("{line}\n").into_bytes();
                managed::send(&mut link, ClientMsg::Input(input)).await?;
            }
            id
        };

        loop {
            self.broker.status(SshPhase::Connected);
            let end = managed::drive(link, id.pane_id, &mut self.cmd_rx, &mut self.size, |msg| {
                (self.publish)(ManagedEvent::Frame(msg));
            })
            .await;
            match end {
                DriveEnd::Exited | DriveEnd::Detached => return Ok(()),
                DriveEnd::KillRequested(result) => return result,
                DriveEnd::Disconnected(error) => {
                    log::debug!("managed SSH pane {} disconnected: {error}", id.pane_id);
                    if !conn.is_alive() {
                        self.manager.evict_connection(conn.key());
                    }
                }
            }

            let Some((next_conn, next_link)) = self.recover(&id).await? else {
                return Ok(());
            };
            conn = next_conn;
            link = next_link;
        }
    }

    async fn recover(
        &mut self,
        id: &ManagedSshPane,
    ) -> anyhow::Result<Option<(Arc<SshConnection>, RemoteLink)>> {
        *self.conn_slot.lock().unwrap() = std::sync::Weak::new();
        let mut wait = Duration::from_millis(500);
        let mut closing = false;
        loop {
            self.broker.status(SshPhase::Connecting);
            let result = {
                let reconnect = reattach(self.manager, self.spec, self.broker, id, self.size, wait);
                tokio::pin!(reconnect);
                loop {
                    tokio::select! {
                        result = &mut reconnect => break result,
                        command = self.cmd_rx.recv() => match command {
                            Some(ChannelCmd::Close) => closing = true,
                            Some(ChannelCmd::Detach) | None => return Ok(None),
                            Some(ChannelCmd::Resize(size)) => self.size = size,
                            // Keystrokes typed during a disconnection must not
                            // run later when an Agent has changed its prompt.
                            Some(ChannelCmd::Data(_) | ChannelCmd::PassiveInput(_)) => {}
                        },
                    }
                }
            };

            match result {
                Ok((conn, mut link, frame)) => {
                    if closing {
                        managed::send(
                            &mut link,
                            ClientMsg::Kill {
                                pane_id: id.pane_id,
                            },
                        )
                        .await?;
                        return Ok(None);
                    }
                    managed::send(&mut link, ClientMsg::Resize(self.size)).await?;
                    *self.conn_slot.lock().unwrap() = Arc::downgrade(&conn);
                    (self.publish)(ManagedEvent::Replay(self.size));
                    (self.publish)(ManagedEvent::Frame(frame));
                    return Ok(Some((conn, link)));
                }
                Err(error) if closing || pane_is_gone(&error) => return Err(error),
                Err(error) => {
                    self.broker.status(SshPhase::Failed {
                        reason: error.to_string(),
                    });
                    wait = (wait * 2).min(Duration::from_secs(10));
                }
            }
        }
    }
}

fn pane_is_gone(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}

async fn reattach(
    manager: &SshManager,
    spec: &NativeSshSpec,
    broker: &Arc<PromptBroker>,
    id: &ManagedSshPane,
    size: WinSize,
    wait: Duration,
) -> anyhow::Result<(Arc<SshConnection>, RemoteLink, DaemonMsg)> {
    tokio::time::sleep(wait).await;
    let (conn, reused) = manager.open_connection(spec, broker).await?;
    let opened = manager.open_existing_pane_link(&conn).await;
    if reused && !conn.is_alive() {
        manager.evict_connection(conn.key());
    }
    let (mut link, version) =
        opened?.ok_or_else(|| anyhow::anyhow!("Hitty Server is unavailable for this session"))?;
    let frame = attach(manager, &conn, &mut link, &version, id, size).await?;
    Ok((conn, link, frame))
}

async fn attach(
    manager: &SshManager,
    conn: &Arc<SshConnection>,
    link: &mut RemoteLink,
    version: &DaemonVersion,
    id: &ManagedSshPane,
    size: WinSize,
) -> anyhow::Result<DaemonMsg> {
    require_instance(version, id)?;
    let frame = managed::attach(link, id.pane_id, size).await?;
    // A daemon restart can reuse a numeric pane ID. The Version exchange uses
    // a separate bridge, so verify again before writing to the attached pane.
    let (_, attached_version) = manager
        .open_existing_pane_link(conn)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Hitty Server is unavailable for this session"))?;
    require_instance(&attached_version, id)?;
    Ok(frame)
}

fn require_instance(version: &DaemonVersion, id: &ManagedSshPane) -> io::Result<()> {
    if version.instance == id.instance {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Hitty Server restarted and no longer hosts this session",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::protocol::{FEATURE_PASSIVE_INPUT, FEATURE_RESIZE_ECHO};
    use crate::daemon::ssh::test_support::base_spec;

    #[test]
    fn can_host_requires_default_terminal_environment() {
        assert!(can_host(&base_spec()));

        let mut without_integration = base_spec();
        without_integration.shell_integration = false;
        assert!(!can_host(&without_integration));

        let mut with_agent_forward = base_spec();
        with_agent_forward.agent_forward = true;
        assert!(!can_host(&with_agent_forward));

        let mut with_x11 = base_spec();
        with_x11.x11 = true;
        assert!(!can_host(&with_x11));

        let mut custom_term = base_spec();
        custom_term.term = "vt100".into();
        assert!(!can_host(&custom_term));
    }

    #[test]
    fn supports_requires_instance_and_required_features() {
        let valid = DaemonVersion {
            protocol: crate::daemon::protocol::PROTOCOL_VERSION,
            build: "0.5.3".into(),
            features: vec![FEATURE_PASSIVE_INPUT.into(), FEATURE_RESIZE_ECHO.into()],
            instance: "server-1".into(),
        };
        assert!(supports(&valid));

        let empty_instance = DaemonVersion {
            instance: String::new(),
            ..valid.clone()
        };
        assert!(!supports(&empty_instance));

        let missing_passive = DaemonVersion {
            features: vec![FEATURE_RESIZE_ECHO.into()],
            ..valid.clone()
        };
        assert!(!supports(&missing_passive));

        let missing_resize = DaemonVersion {
            features: vec![FEATURE_PASSIVE_INPUT.into()],
            ..valid
        };
        assert!(!supports(&missing_resize));
    }

    #[test]
    fn require_instance_marks_restart_mismatch_as_gone() {
        let id = ManagedSshPane {
            instance: "server-1".into(),
            pane_id: 42,
        };
        let matching = DaemonVersion {
            protocol: crate::daemon::protocol::PROTOCOL_VERSION,
            build: "0.5.3".into(),
            features: vec![],
            instance: "server-1".into(),
        };
        assert!(require_instance(&matching, &id).is_ok());

        let restarted = DaemonVersion {
            instance: "server-2".into(),
            ..matching
        };
        let err = require_instance(&restarted, &id).unwrap_err();
        assert!(pane_is_gone(&anyhow::Error::from(err)));
        assert!(!pane_is_gone(&anyhow::anyhow!("transient failure")));
    }
}

use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::core::machine::{observe_pane, observed_store};
use crate::core::osc::OscTokenizer;
use crate::daemon::protocol::DaemonMsg;
use crate::daemon::ssh::ManagedEvent;
use crate::daemon::terminal_activity::OutputActivity;

use super::{
    OutputGate, PaneState, ReplayRing, ShellState, fan_out_one, note_terminal_activity, notify,
    observed_facts, parse_osc_title, resize_state, write_ready_ssh_resume,
};

/// Mirror the owning daemon's terminal stream into the local workspace pane.
/// The remote daemon reports Agent facts from its own PTY and hooks.
pub(super) struct ManagedSshProxy {
    state: Arc<Mutex<PaneState>>,
    gate: Arc<OutputGate>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    title: OscTokenizer,
    output_activity: Option<OutputActivity>,
}

impl ManagedSshProxy {
    pub(super) fn new(
        state: Arc<Mutex<PaneState>>,
        gate: Arc<OutputGate>,
        writer: Arc<Mutex<Box<dyn Write + Send>>>,
    ) -> Self {
        Self {
            state,
            gate,
            writer,
            title: OscTokenizer::new(&[b"0", b"2"]),
            output_activity: None,
        }
    }

    pub(super) fn accept(&mut self, event: ManagedEvent) {
        match event {
            ManagedEvent::Bound(backing) => {
                let mut st = self.state.lock().unwrap();
                st.managed_ssh = Some(backing.clone());
                notify(&mut st, DaemonMsg::ManagedSsh(backing.clone()));
                let pane = st.id;
                drop(st);
                if let Some(store) = observed_store() {
                    store.note_pane_facts(pane, |record| {
                        if let Some(spec) = record.ssh_spec.as_mut() {
                            spec.managed = Some(backing);
                        }
                    });
                    store.flush();
                }
            }
            ManagedEvent::Replay(size) => {
                self.title = OscTokenizer::new(&[b"0", b"2"]);
                self.output_activity = None;
                let mut st = self.state.lock().unwrap();
                st.ring = ReplayRing::new(size);
                st.osc_title = None;
                st.shell = ShellState::default();
                st.agent = None;
                st.agent_session = None;
                st.agent_input_suspended = false;
                st.foreground_app = None;
                notify(&mut st, DaemonMsg::Snapshot(b"\x1bc\x1b[3J".to_vec()));
                notify(&mut st, DaemonMsg::Agent(None));
                notify(&mut st, DaemonMsg::AgentStatus(None));
                notify(&mut st, DaemonMsg::AgentInputSuspended(false));
                notify(&mut st, DaemonMsg::ForegroundApp(None));
                let pane = st.id;
                drop(st);
                observe_pane(pane, |record| {
                    record.osc_title = None;
                    record.agent = None;
                    record.foreground_app = None;
                });
            }
            ManagedEvent::Frame(msg) => self.frame(msg),
        }
    }

    fn frame(&mut self, msg: DaemonMsg) {
        if self.gate.queued_bytes() >= OutputGate::HIGH_WATER {
            tokio::task::block_in_place(|| self.gate.wait_below_high_water());
        }

        let mut st = self.state.lock().unwrap();
        let mut facts_changed = true;
        let previous_activity = st.last_terminal_activity_ms;
        match &msg {
            DaemonMsg::Size(size) => {
                resize_state(&mut st, *size);
                return;
            }
            DaemonMsg::Output(bytes) | DaemonMsg::Snapshot(bytes) => {
                let mut title = None;
                self.title.feed(bytes, |osc| {
                    if let Some(found) = parse_osc_title(osc) {
                        title = Some(found);
                    }
                });
                if let Some(title) = title {
                    st.osc_title = (!title.is_empty()).then_some(title);
                } else {
                    facts_changed = false;
                }
                st.ring.append(bytes);
                if matches!(msg, DaemonMsg::Output(_)) && st.agent.is_some() {
                    let size = st.ring.tail_size();
                    let activity = self
                        .output_activity
                        .get_or_insert_with(|| OutputActivity::new(size));
                    if activity.feed(bytes, size) {
                        note_terminal_activity(&mut st);
                    }
                }
            }
            DaemonMsg::Cwd(cwd) => st.cwd = Some(cwd.clone()),
            DaemonMsg::Prompt {
                active,
                at_prompt,
                last_exit,
            } => {
                st.shell.active = *active;
                st.shell.at_prompt = *at_prompt;
                st.shell.last_exit_code = *last_exit;
                write_ready_ssh_resume(&mut st, &self.writer);
                facts_changed = false;
            }
            DaemonMsg::Agent(agent) => {
                st.agent = *agent;
                st.agent_session = None;
                self.output_activity = None;
            }
            DaemonMsg::AgentStatus(session) => st.agent_session = session.clone(),
            DaemonMsg::ForegroundApp(app) => st.foreground_app = *app,
            DaemonMsg::AgentInputSuspended(suspended) => {
                st.agent_input_suspended = *suspended;
                facts_changed = false;
            }
            DaemonMsg::Image(_) | DaemonMsg::DeleteImage(_) => facts_changed = false,
            DaemonMsg::Exited { code } => {
                // The local pane reader reports exit once, after the bridge
                // finishes. Preserve the status from the owning daemon.
                st.exit_code = *code;
                return;
            }
            _ => return,
        }

        match &msg {
            DaemonMsg::Output(bytes) | DaemonMsg::Image(bytes) => {
                let length = bytes.len();
                fan_out_one(&mut st, msg, length, &self.gate);
            }
            _ => notify(&mut st, msg),
        }

        let pane = st.id;
        let facts = facts_changed.then(|| (observed_facts(&st), st.foreground_app));
        let activity = st.last_terminal_activity_ms;
        drop(st);
        if facts.is_some() || activity != previous_activity {
            observe_pane(pane, |record| {
                if let Some((facts, foreground)) = facts {
                    record.cwd = facts.cwd;
                    record.osc_title = facts.osc_title;
                    record.agent = facts.agent;
                    record.foreground_app = foreground;
                }
                record.note_activity(None, activity);
            });
        }
    }
}

//! What the agent in a pane is doing, as the window would show it.
//!
//! The server knows what an agent's hooks reported. The window knows more: it
//! also reads the agent's screen, which is the only word there is from an agent
//! with no hooks, and the only place a permission prompt shows for one whose
//! hooks do not announce it. A command line that typed into a pane on the
//! hooks' word alone would type over a prompt the user can plainly see — so
//! this takes the same look, with the same rules, and reconciles the two the
//! same way.

use anyhow::Result;
use hitty_core::core::cli_agent::{AgentSessionState, AgentStatus, CLIAgent};
use hitty_core::core::screen_status::{self, Tracker};

use crate::backend::Backend;
use crate::screen;

/// One pane's screen, watched over successive looks. Kept across the polls of
/// a wait, because "idle after working" is a finished turn and a single look
/// cannot tell that from an agent that never started.
#[derive(Default)]
pub struct ScreenReader {
    tracker: Tracker,
    looks: u64,
}

impl ScreenReader {
    /// Look at the pane and say what its agent is doing: the hooks' report and
    /// the screen's reading, merged as the window merges them.
    ///
    /// The hooks' word alone for an agent with no screen rules, and when the
    /// pane has nothing to show yet. `None` when neither source says anything.
    pub fn status(
        &mut self,
        backend: &mut dyn Backend,
        pane: u64,
        agent: Option<CLIAgent>,
        title: &str,
        hook: Option<&AgentSessionState>,
    ) -> Result<Option<AgentStatus>> {
        let Some(agent) = agent.filter(|a| screen_status::reads(*a)) else {
            return Ok(hook.map(|s| s.status));
        };
        if let Some(segment) = backend.capture(pane, false)?.last() {
            self.looks += 1;
            // Every look counts as new output: finding out whether the screen
            // moved costs the capture that reading it does.
            self.tracker.observe(
                Some(agent),
                self.looks,
                title,
                &screen_status::last_progress(&segment.bytes),
                || screen::detection_text(segment),
            );
        }
        Ok(screen_status::merge(agent, hook, self.tracker.status()))
    }
}

pub fn name(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Idle => "idle",
        AgentStatus::Working => "working",
        AgentStatus::Waiting => "waiting",
        AgentStatus::Done => "done",
    }
}

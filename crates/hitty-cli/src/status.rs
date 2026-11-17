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
use hitty_core::core::osc;
use hitty_core::core::screen_status::{self, Tracker};

use crate::backend::{Backend, CaptureSegment};
use crate::screen;

/// One pane's screen, watched over successive looks. Kept across the polls of
/// a wait, because "idle after working" is a finished turn and a single look
/// cannot tell that from an agent that never started.
pub struct ScreenReader {
    pane: u64,
    agent: Option<CLIAgent>,
    tracker: Tracker,
    looks: u64,
    /// What the last look captured, so its text can be had without a second
    /// capture. Empty when that look did not read the screen.
    seen: Vec<CaptureSegment>,
}

impl ScreenReader {
    pub fn new(pane: u64, agent: Option<CLIAgent>) -> ScreenReader {
        ScreenReader {
            pane,
            agent,
            tracker: Tracker::default(),
            looks: 0,
            seen: Vec::new(),
        }
    }

    /// Look at the pane and say what its agent is doing: the hooks' report and
    /// the screen's reading, merged as the window merges them.
    ///
    /// The hooks' word alone for an agent with no screen rules, and when the
    /// pane cannot be read — a look that fails must not fail the caller, who
    /// still has the hooks. `None` when neither source says anything.
    pub fn status(
        &mut self,
        backend: &mut dyn Backend,
        title: &str,
        hook: Option<&AgentSessionState>,
    ) -> Option<AgentStatus> {
        self.seen.clear();
        let Some(agent) = self.agent.filter(|a| screen_status::has_manifest(*a)) else {
            return hook.map(|s| s.status);
        };
        self.seen = backend.capture(self.pane, false).unwrap_or_default();
        if let Some(segment) = self.seen.last() {
            self.looks += 1;
            // Every look counts as new output: finding out whether the screen
            // moved costs the capture that reading it does. A capture takes
            // some 300 ms to settle, which is the pace the tracker's idle
            // hold was tuned for.
            self.tracker.observe(
                Some(agent),
                self.looks,
                title,
                &osc::last_progress(&segment.bytes),
                || screen::detection_text(segment),
            );
        }
        screen_status::merge(agent, hook, self.tracker.status())
    }

    /// What the screen alone has shown, the hooks left out of it.
    pub fn screen(&self) -> Option<AgentStatus> {
        self.tracker.status().and_then(|s| s.status)
    }

    /// The pane's text as of the last look, captured now if that look did not
    /// read the screen.
    pub fn text(&mut self, backend: &mut dyn Backend) -> Result<String> {
        if self.seen.is_empty() {
            self.seen = backend.capture(self.pane, false)?;
        }
        Ok(screen::render(&self.seen))
    }
}

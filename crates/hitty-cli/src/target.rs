//! Naming a pane the way a person does.
//!
//! `%52` is what the server calls a pane. Nobody looking at the window does:
//! they say "the one on the right", or "Claude". An agent asked to talk to
//! *the Claude on the right* should not have to read a layout tree to find out
//! that means `%52` — and if it has to work that out itself, two agents will
//! work it out two ways. So the words resolve here, against the same layout
//! rule the window moves focus by.

use anyhow::{Result, anyhow, bail};
use hitty_core::core::cli_agent::CLIAgent;
use hitty_core::core::layout::{self, Direction};
use hitty_core::core::machine::{Machine, Tab, Workspace};
use hitty_core::daemon::control::PaneAgentState;

use crate::address::{self, Context};
use crate::backend::Backend;
use crate::commands::{agent_states, fetch_machine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Pane(u64),
    /// The pane on that side of the caller's own.
    Direction(Direction),
    /// The one pane running that agent, nearest first.
    Agent(CLIAgent),
}

pub fn parse(s: &str) -> Result<Target> {
    let word = s.trim().to_ascii_lowercase();
    if let Some(dir) = Direction::ALL.into_iter().find(|d| d.name() == word) {
        return Ok(Target::Direction(dir));
    }
    if let Ok(pane) = address::parse_pane(s) {
        return Ok(Target::Pane(pane));
    }
    // A `%` is someone writing a pane address, so the address error is the
    // useful one; `%claude` was never going to be an agent.
    if !s.starts_with('%')
        && let Some(agent) = CLIAgent::from_slug(&word)
    {
        return Ok(Target::Agent(agent));
    }
    bail!(
        "'{s}' is not a pane — name one as %42, as left/right/up/down from this pane, \
         or by the agent running in it (claude, codex, …)"
    )
}

/// What the server knows that a target is resolved against: one snapshot of
/// each, so a pane cannot be on the right in one answer and gone in the next.
pub struct Scene {
    pub machine: Machine,
    pub states: Vec<PaneAgentState>,
    /// The calling pane, when the CLI runs inside one.
    pub me: Option<u64>,
}

impl Scene {
    pub fn fetch(ctx: &Context, backend: &mut dyn Backend) -> Result<Scene> {
        Ok(Scene {
            machine: fetch_machine(backend)?,
            states: agent_states(backend)?,
            // A broken `HITTY_PANE` is reported where it is needed — by a
            // direction, which cannot work without it — not on every call.
            me: ctx
                .pane
                .as_deref()
                .and_then(|pane| address::parse_pane(pane).ok()),
        })
    }

    pub fn state(&self, pane: u64) -> Option<&PaneAgentState> {
        self.states.iter().find(|s| s.pane_id == pane)
    }

    /// The agent in a pane: what its hooks say, else what the server saw
    /// running. The second covers an agent whose hooks are not installed —
    /// it has a name even though it reports no status.
    pub fn agent(&self, pane: u64) -> Option<CLIAgent> {
        self.state(pane).and_then(|s| s.agent).or_else(|| {
            self.machine
                .panes
                .iter()
                .find(|p| p.id == pane)
                .and_then(|p| p.agent.as_ref())
                .map(|facts| facts.agent)
        })
    }

    pub fn is_live(&self, pane: u64) -> bool {
        self.machine.panes.iter().any(|p| p.id == pane && p.live)
    }

    pub fn place(&self, pane: u64) -> Option<(&Workspace, &Tab)> {
        self.machine.workspaces.iter().find_map(|ws| {
            ws.tabs
                .iter()
                .find(|tab| tab.root.contains(pane))
                .map(|tab| (ws, tab))
        })
    }

    /// Which side of the caller `pane` is on, if it is the pane a direction
    /// would name. A pane two splits away has no side: `right` means one pane.
    pub fn side_of_me(&self, pane: u64) -> Option<Direction> {
        let me = self.me?;
        let (_, tab) = self.place(me)?;
        Direction::ALL
            .into_iter()
            .find(|dir| layout::neighbor_pane(&tab.root, me, *dir) == Some(pane))
    }

    pub fn resolve(&self, target: Target) -> Result<u64> {
        match target {
            Target::Pane(pane) => Ok(pane),
            Target::Direction(dir) => {
                let me = self.me.ok_or_else(|| {
                    anyhow!(
                        "'{}' is relative to the calling pane, and this is not a hitty shell \
                         — name the pane as %42",
                        dir.name()
                    )
                })?;
                let (_, tab) = self.place(me).ok_or_else(|| {
                    anyhow!("this pane (%{me}) is in no tab — name the pane as %42")
                })?;
                layout::neighbor_pane(&tab.root, me, dir).ok_or_else(|| {
                    anyhow!(
                        "there is no pane {} %{me} — `hitty peers` shows what is around it",
                        match dir {
                            Direction::Left => "left of",
                            Direction::Right => "right of",
                            Direction::Up => "above",
                            Direction::Down => "below",
                        }
                    )
                })
            }
            Target::Agent(agent) => self.resolve_agent(agent),
        }
    }

    /// The pane running `agent`, looking outward from the caller: its own tab,
    /// then its workspace, then the machine. The first circle that holds any
    /// decides — one there is the answer, two is a question for the user.
    /// Never the caller itself: "ask Claude", said to Claude, means another.
    fn resolve_agent(&self, agent: CLIAgent) -> Result<u64> {
        let runs = |pane: &u64| {
            Some(*pane) != self.me && self.is_live(*pane) && self.agent(*pane) == Some(agent)
        };
        let home = self.me.and_then(|me| self.place(me));
        let tab: Vec<u64> = home.map(|(_, tab)| tab.root.pane_ids()).unwrap_or_default();
        let workspace: Vec<u64> = home
            .map(|(ws, _)| ws.tabs.iter().flat_map(|t| t.root.pane_ids()).collect())
            .unwrap_or_default();
        let machine: Vec<u64> = self
            .machine
            .workspaces
            .iter()
            .flat_map(|ws| ws.tabs.iter().flat_map(|t| t.root.pane_ids()))
            .collect();
        for circle in [tab, workspace, machine] {
            let found: Vec<u64> = circle.into_iter().filter(runs).collect();
            match found.as_slice() {
                [] => continue,
                [one] => return Ok(*one),
                many => bail!(
                    "{} panes are running {}: {} — say which, by address or by side \
                     (`hitty peers` shows where each one is)",
                    many.len(),
                    agent.display_name(),
                    many.iter()
                        .map(|p| format!("%{p}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        bail!(
            "no other pane is running {} — `hitty peers` lists the agents that are here",
            agent.display_name()
        )
    }
}

/// The pane an address-taking verb means: the address given, in any of its
/// shapes, or the calling pane when none is.
///
/// A plain `%42` — and no address at all — is answered without asking the
/// server anything, exactly as before these verbs learned words.
pub fn pane_or_context(
    explicit: Option<&str>,
    ctx: &Context,
    backend: &mut dyn Backend,
) -> Result<u64> {
    match explicit.map(parse).transpose()? {
        None => address::pane_or_context(None, ctx),
        Some(Target::Pane(pane)) => Ok(pane),
        Some(target) => Scene::fetch(ctx, backend)?.resolve(target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hitty_core::core::cli_agent::{AgentSessionState, AgentStatus};
    use hitty_core::core::machine::{AgentFacts, Axis, PaneNode, PaneRecord, TabId};
    use hitty_core::core::session::WorkspaceId;

    fn leaf(pane: u64) -> Box<PaneNode> {
        Box::new(PaneNode::Leaf { pane })
    }

    fn tab(root: Box<PaneNode>) -> Tab {
        Tab {
            id: TabId::new(),
            name: None,
            sidebar_group: None,
            focused_pane: None,
            root: *root,
        }
    }

    fn reporting(pane_id: u64, agent: CLIAgent) -> PaneAgentState {
        PaneAgentState {
            pane_id,
            agent: Some(agent),
            state: AgentSessionState {
                status: AgentStatus::Done,
                ..Default::default()
            },
            reply: None,
            turns: 0,
        }
    }

    /// One workspace: a tab with `47 | 52`, and a second tab holding `60`.
    /// A second workspace holds `70`. Claude reports from 52, 60 and 70.
    fn scene(me: Option<u64>) -> Scene {
        let home = Workspace {
            id: WorkspaceId::new(),
            tabs: vec![
                tab(Box::new(PaneNode::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.58,
                    a: leaf(47),
                    b: leaf(52),
                })),
                tab(leaf(60)),
            ],
            ..Default::default()
        };
        let away = Workspace {
            id: WorkspaceId::new(),
            tabs: vec![tab(leaf(70))],
            ..Default::default()
        };
        let mut panes: Vec<PaneRecord> = [47, 52, 60, 70]
            .into_iter()
            .map(|id| PaneRecord {
                live: true,
                ..PaneRecord::new(id)
            })
            .collect();
        // Codex in 47 has no hooks: the server saw the process, nothing more.
        panes[0].agent = Some(AgentFacts {
            agent: CLIAgent::Codex,
            session_id: None,
            launch_argv: None,
            status: None,
            restore_pending: false,
            unstarted: false,
        });
        Scene {
            machine: Machine {
                workspaces: vec![home, away],
                panes,
            },
            states: vec![
                reporting(52, CLIAgent::Claude),
                reporting(60, CLIAgent::Claude),
                reporting(70, CLIAgent::Claude),
            ],
            me,
        }
    }

    #[test]
    fn the_three_ways_to_name_a_pane_parse_apart() {
        assert_eq!(parse("%52").unwrap(), Target::Pane(52));
        assert_eq!(parse("52").unwrap(), Target::Pane(52));
        assert_eq!(parse("right").unwrap(), Target::Direction(Direction::Right));
        assert_eq!(parse("Up").unwrap(), Target::Direction(Direction::Up));
        assert_eq!(parse("claude").unwrap(), Target::Agent(CLIAgent::Claude));
        assert_eq!(parse("Codex").unwrap(), Target::Agent(CLIAgent::Codex));

        let err = parse("the-other-one").unwrap_err().to_string();
        assert!(err.contains("left/right/up/down"), "{err}");
        assert!(
            parse("%claude").is_err(),
            "a marked address is never an agent name"
        );
    }

    #[test]
    fn a_direction_is_the_pane_the_window_would_move_focus_to() {
        let from_left = scene(Some(47));
        assert_eq!(
            from_left
                .resolve(Target::Direction(Direction::Right))
                .unwrap(),
            52
        );
        let err = from_left
            .resolve(Target::Direction(Direction::Left))
            .unwrap_err()
            .to_string();
        assert!(err.contains("left of %47"), "{err}");
        assert!(err.contains("hitty peers"), "{err}");

        assert_eq!(from_left.side_of_me(52), Some(Direction::Right));
        assert_eq!(from_left.side_of_me(60), None, "another tab has no side");
    }

    #[test]
    fn a_direction_needs_a_pane_to_be_relative_to() {
        let err = scene(None)
            .resolve(Target::Direction(Direction::Right))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a hitty shell"), "{err}");
    }

    #[test]
    fn an_agent_name_means_the_nearest_pane_running_it() {
        // From 47, three panes run Claude; the one sharing its tab is meant.
        assert_eq!(
            scene(Some(47))
                .resolve(Target::Agent(CLIAgent::Claude))
                .unwrap(),
            52
        );
        // From 52 — itself Claude — the name means another: the only one left
        // in its workspace, not the one in somebody else's.
        assert_eq!(
            scene(Some(52))
                .resolve(Target::Agent(CLIAgent::Claude))
                .unwrap(),
            60
        );
        // An agent the server only saw as a process still has a name.
        assert_eq!(
            scene(Some(52))
                .resolve(Target::Agent(CLIAgent::Codex))
                .unwrap(),
            47
        );
    }

    #[test]
    fn two_equally_near_agents_are_a_question_not_a_guess() {
        // Outside any pane nothing is nearer than anything else.
        let err = scene(None)
            .resolve(Target::Agent(CLIAgent::Claude))
            .unwrap_err()
            .to_string();
        assert!(err.contains("3 panes are running Claude"), "{err}");
        assert!(err.contains("%52, %60, %70"), "{err}");

        let err = scene(Some(47))
            .resolve(Target::Agent(CLIAgent::Gemini))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no other pane is running Gemini"), "{err}");
    }
}

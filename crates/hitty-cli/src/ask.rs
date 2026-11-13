//! Talking to an agent that is already there.
//!
//! `send`, `wait` and `capture` are enough to hold a conversation with another
//! pane's agent, and stringing them together correctly is a page of the skill:
//! find the pane, check it is not mid-turn or stopped at a permission prompt,
//! type without the Enter being swallowed, wait for *this* turn rather than the
//! last one, then read an answer back off a screen that has since scrolled.
//! Each step has a way to go quietly wrong, and the user asked for one thing:
//! "have the Claude on the right look at this".
//!
//! `ask` is that one thing. `peers` is the look around that precedes it.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use hitty_core::core::cli_agent::AgentStatus;
use hitty_core::daemon::control::PaneAgentState;
use serde_json::{Value, json};

use crate::address::Context;
use crate::backend::Backend;
use crate::cli::AskArgs;
use crate::commands::{Outcome, Report, agent_states, pane_is_live};
use crate::output;
use crate::screen;
use crate::target::{self, Scene};

/// The asked agent stopped for its human — a permission prompt, a question —
/// instead of answering. Its own code, because neither neighbour fits: nothing
/// failed, and waiting longer will not help.
pub const EXIT_NEEDS_USER: i32 = 3;

/// How much of a screen stands in for an answer the agent did not report.
const SCREEN_TAIL_LINES: usize = 40;

fn status_name(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Idle => "idle",
        AgentStatus::Working => "working",
        AgentStatus::Waiting => "waiting",
        AgentStatus::Done => "done",
    }
}

pub fn peers(ctx: &Context, backend: &mut dyn Backend) -> Result<Outcome> {
    let scene = Scene::fetch(ctx, backend)?;
    let home = scene.me.and_then(|me| scene.place(me)).map(|(_, tab)| tab);

    let mut rows = Vec::new();
    let mut list = Vec::new();
    for ws in &scene.machine.workspaces {
        for tab in &ws.tabs {
            let ordinal = crate::resolve::ordinal_of(&scene.machine, tab.id).unwrap_or(0);
            let here = home.is_some_and(|home| home.id == tab.id);
            for pane in tab.root.pane_ids() {
                let agent = scene.agent(pane);
                // Around the caller every pane is worth seeing — the one on
                // the right may be a plain shell. Further off, only agents:
                // this is a list of who can be talked to, not `pane ls`.
                if Some(pane) == scene.me || !scene.is_live(pane) || (!here && agent.is_none()) {
                    continue;
                }
                let side = scene.side_of_me(pane);
                let status = scene.state(pane).map(|s| status_name(s.state.status));
                let record = scene.machine.panes.iter().find(|p| p.id == pane);
                let title = record
                    .and_then(|p| p.osc_title.clone())
                    .filter(|t| !t.trim().is_empty());
                let cwd = record.and_then(|p| p.cwd.clone());
                let place = match (side, here) {
                    (Some(side), _) => side.name().to_string(),
                    (None, true) => "this tab".to_string(),
                    (None, false) => format!("@{ordinal}"),
                };
                rows.push(vec![
                    format!("%{pane}"),
                    place,
                    agent.map_or("-", |a| a.slug()).to_string(),
                    // An agent with no status is one whose hooks are missing:
                    // it can be read, but `ask` cannot tell when it answers.
                    status.unwrap_or("-").to_string(),
                    title
                        .clone()
                        .or_else(|| cwd.as_deref().map(|c| output::path_leaf(c).to_string()))
                        .unwrap_or_else(|| "-".to_string()),
                ]);
                list.push(json!({
                    "pane": pane,
                    "direction": side.map(|s| s.name()),
                    "same_tab": here,
                    "tab": ordinal,
                    "agent": agent.map(|a| a.slug()),
                    "status": status,
                    "title": title,
                    "cwd": cwd,
                }));
            }
        }
    }
    let human = if rows.is_empty() {
        "no other panes around this one, and no agents elsewhere\n".to_string()
    } else {
        output::table(&["PANE", "WHERE", "AGENT", "STATUS", "TITLE"], &rows)
    };
    Ok(Outcome::Report(Report {
        human,
        json: json!({ "pane": scene.me, "peers": list }),
    }))
}

/// The message as the other agent receives it: who is asking, then what.
///
/// The first line is for the reader on the far side. Without it an agent
/// handed a question out of nowhere either treats it as its own user's — and
/// may act with that user's authority — or goes looking for a way to write
/// back, which is how two agents end up typing into each other forever. It is
/// told both things: who this is from, and that answering here is enough.
fn envelope(scene: &Scene, message: &str) -> String {
    let from = match scene.me {
        Some(me) => match scene.agent(me) {
            Some(agent) => format!("the {} agent in pane %{me}", agent.display_name()),
            None => format!("pane %{me}"),
        },
        None => "a hitty command line outside this window".to_string(),
    };
    format!(
        "[hitty ask — from {from}, sent on its user's request. Answer here as usual: your \
         reply is returned to it automatically, so do not message that pane yourself.]\n\n{message}"
    )
}

/// The bytes that put `text` into an agent's input box as one paste.
///
/// Bracketed, because the message has newlines and an agent's editor submits
/// on a bare one: typed as keystrokes, the first line would go out alone. The
/// escape byte is dropped on the way so a message cannot close the bracket
/// early and have the rest of itself read as keys.
fn paste_bytes(text: &str) -> Vec<u8> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut bytes = b"\x1b[200~".to_vec();
    bytes.extend(text.bytes().filter(|&b| b != 0x1b));
    bytes.extend_from_slice(b"\x1b[201~");
    bytes
}

/// What stands in for an answer the agent's hooks did not carry: the tail of
/// its screen. Marked as such in the report — it is the view, with the input
/// box and status line in it, not the text.
fn screen_tail(backend: &mut dyn Backend, pane: u64) -> Result<String> {
    let text = screen::render(&backend.capture(pane, false)?);
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(SCREEN_TAIL_LINES);
    Ok(lines[from..].join("\n"))
}

pub fn ask(args: AskArgs, ctx: &Context, backend: &mut dyn Backend) -> Result<Outcome> {
    // The same gap `send` leaves before a key: a raw-mode editor reads a paste
    // and an Enter that arrive together as one paste, and keeps the newline.
    const SUBMIT_GAP: Duration = Duration::from_millis(200);
    /// See `wait`: a reporting agent can outlive its pane.
    const LIVENESS_EVERY: u32 = 4;

    let message = args.message.trim();
    if message.is_empty() {
        bail!("ask needs a message to send");
    }
    let scene = Scene::fetch(ctx, backend)?;
    let pane = scene.resolve(target::parse(&args.target)?)?;
    if Some(pane) == scene.me {
        bail!("%{pane} is this pane — ask is for talking to another one");
    }
    if !scene.is_live(pane) {
        bail!("no live pane %{pane} on this machine — `hitty peers` lists who is around");
    }
    // Everything `ask` promises rests on the agent reporting: without a status
    // there is no telling whether it is safe to type, nor when it has answered.
    let Some(before) = scene.state(pane).cloned() else {
        bail!(
            "nothing in pane %{pane} reports agent status — it is a plain shell, or an agent \
             whose hooks are not installed (`hitty doctor` says which). `hitty send` types \
             into a pane like that; `ask` needs an agent that can say when it is done"
        );
    };
    let agent = scene.agent(pane);
    let name = agent.map_or("the agent", |a| a.display_name());
    match before.state.status {
        // Typing into a turn in progress queues behind it at best, and the
        // answer that comes back first belongs to somebody else's question.
        AgentStatus::Working => bail!(
            "{name} in pane %{pane} is in the middle of a turn — wait for it \
             (`hitty wait %{pane} --until done,waiting`) and ask again"
        ),
        // Here keystrokes are the answer to whatever it is asking: a pasted
        // message and an Enter could approve a command nobody read.
        AgentStatus::Waiting => bail!(
            "{name} in pane %{pane} is waiting on its user{} — that prompt is theirs to \
             answer, so nothing was sent",
            before
                .state
                .message
                .as_deref()
                .map(|m| format!(" ({m})"))
                .unwrap_or_default()
        ),
        AgentStatus::Idle | AgentStatus::Done => {}
    }

    backend.send_input(pane, paste_bytes(&envelope(&scene, message)))?;
    std::thread::sleep(SUBMIT_GAP);
    backend.send_input(pane, b"\r".to_vec())?;

    let base = json!({
        "pane": pane,
        "agent": agent.map(|a| a.slug()),
        "sent": message,
    });
    let finish = |code: i32, human: String, state: Option<&PaneAgentState>, more: Value| {
        let mut json = base.clone();
        json["status"] = state.map(|s| status_name(s.state.status)).into();
        json["session_id"] = state.and_then(|s| s.state.session_id.clone()).into();
        for (key, value) in more.as_object().into_iter().flatten() {
            json[key] = value.clone();
        }
        let report = Report { human, json };
        Ok(match code {
            0 => Outcome::Report(report),
            code => Outcome::Exit(code, report),
        })
    };
    if args.no_wait {
        return finish(
            0,
            format!("sent to %{pane}"),
            Some(&before),
            json!({ "answered": false }),
        );
    }

    let deadline = Instant::now().checked_add(Duration::from_secs(args.timeout));
    let interval = Duration::from_millis(args.interval);
    let mut seen_working = false;
    let mut polls: u32 = 0;
    loop {
        let nap = match deadline {
            Some(d) => interval.min(d.saturating_duration_since(Instant::now())),
            None => interval,
        };
        std::thread::sleep(nap);

        let states = agent_states(backend)?;
        polls += 1;
        let now = states.into_iter().find(|s| s.pane_id == pane);
        let gone = match &now {
            None => true,
            Some(_) => polls.is_multiple_of(LIVENESS_EVERY) && !pane_is_live(backend, pane)?,
        };
        let Some(now) = now.filter(|_| !gone) else {
            let human = format!("{name} in pane %{pane} ended before answering");
            eprintln!("hitty: {human}");
            return finish(1, human, None, json!({ "answered": false }));
        };
        seen_working |= now.state.status == AgentStatus::Working;

        // The turn count is the edge; the status is only a level. A short
        // answer can start and finish between two polls and leave `done`
        // standing where it stood — indistinguishable from never having
        // started, except that a turn was counted. A server from before the
        // count existed never moves it, and is read by the level instead:
        // seen working, now done.
        let answered =
            now.turns > before.turns || (seen_working && now.state.status == AgentStatus::Done);
        if answered {
            let (reply, source) = match now.reply.as_deref().filter(|r| !r.trim().is_empty()) {
                Some(reply) => (reply.to_string(), "agent"),
                None => (screen_tail(backend, pane)?, "screen"),
            };
            let human = match source {
                "agent" => reply.clone(),
                _ => format!(
                    "({name} finished without reporting its answer as text — this is the \
                     end of pane %{pane}'s screen instead)\n{reply}"
                ),
            };
            return finish(
                0,
                human,
                Some(&now),
                json!({ "answered": true, "reply": reply, "reply_source": source }),
            );
        }
        // It was not waiting when the message went in, so this is a stop the
        // message caused — a permission prompt, a question back.
        if now.state.status == AgentStatus::Waiting {
            let asks = now.state.message.clone();
            let human = format!(
                "{name} in pane %{pane} stopped to ask its user{} — that is theirs to answer; \
                 `hitty wait %{pane} --until done --json` picks the reply up afterwards",
                asks.as_deref()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            );
            eprintln!("hitty: {human}");
            return finish(
                EXIT_NEEDS_USER,
                human,
                Some(&now),
                json!({ "answered": false, "message": asks }),
            );
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            let human = format!(
                "{name} in pane %{pane} is still {} after {}s — the message was delivered; \
                 `hitty wait %{pane} --until done,waiting --json` waits on and carries the reply",
                status_name(now.state.status),
                args.timeout
            );
            eprintln!("hitty: {human}");
            return finish(
                124,
                human,
                Some(&now),
                json!({ "answered": false, "timed_out": true }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::CaptureSegment;
    use crate::backend::mock::MockBackend;
    use crate::cli::Cli;
    use crate::commands::execute;
    use clap::Parser as _;
    use hitty_core::core::cli_agent::{AgentSessionState, CLIAgent};
    use hitty_core::core::machine::{Axis, Machine, PaneNode, PaneRecord, Tab, TabId, Workspace};
    use hitty_core::daemon::control::ReplyOk;
    use hitty_core::daemon::protocol::WinSize;

    /// `47 | 52`: the caller on the left, Claude on the right.
    fn backend() -> MockBackend {
        let ws = Workspace {
            tabs: vec![Tab {
                id: TabId::new(),
                name: None,
                sidebar_group: None,
                focused_pane: Some(47),
                root: PaneNode::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.58,
                    a: Box::new(PaneNode::Leaf { pane: 47 }),
                    b: Box::new(PaneNode::Leaf { pane: 52 }),
                },
            }],
            ..Default::default()
        };
        let panes = [47, 52]
            .into_iter()
            .map(|id| PaneRecord {
                live: true,
                ..PaneRecord::new(id)
            })
            .collect();
        MockBackend::with_machine(Machine {
            workspaces: vec![ws],
            panes,
        })
    }

    fn ctx() -> Context {
        Context {
            pane: Some("47".into()),
            ..Context::default()
        }
    }

    fn claude(status: AgentStatus, turns: u64, reply: Option<&str>) -> PaneAgentState {
        PaneAgentState {
            pane_id: 52,
            agent: Some(CLIAgent::Claude),
            state: AgentSessionState {
                status,
                session_id: Some("sess-52".into()),
                ..Default::default()
            },
            reply: reply.map(String::from),
            turns,
        }
    }

    fn codex_caller() -> PaneAgentState {
        PaneAgentState {
            pane_id: 47,
            agent: Some(CLIAgent::Codex),
            state: AgentSessionState {
                status: AgentStatus::Working,
                ..Default::default()
            },
            reply: None,
            turns: 0,
        }
    }

    fn script(backend: &mut MockBackend, polls: Vec<Vec<PaneAgentState>>) {
        for states in polls {
            backend.replies.push_back(ReplyOk::AgentStates(states));
        }
    }

    fn run(args: &[&str], backend: &mut MockBackend) -> Result<Outcome> {
        let mut argv = vec!["hitty"];
        argv.extend_from_slice(args);
        execute(Cli::try_parse_from(argv).unwrap(), &ctx(), backend)
    }

    fn json_of(outcome: Outcome) -> (i32, Value) {
        match outcome {
            Outcome::Report(report) => (0, report.json),
            Outcome::Exit(code, report) => (code, report.json),
        }
    }

    #[test]
    fn ask_sends_one_paste_then_enter_and_returns_the_agents_own_answer() {
        let mut backend = backend();
        script(
            &mut backend,
            vec![
                vec![codex_caller(), claude(AgentStatus::Done, 3, Some("old"))],
                vec![codex_caller(), claude(AgentStatus::Working, 3, None)],
                vec![
                    codex_caller(),
                    claude(AgentStatus::Done, 4, Some("Two issues:\n1. off by one")),
                ],
            ],
        );
        let outcome = run(
            &[
                "ask",
                "right",
                "Review the diff.\nBe brief.",
                "--interval",
                "50",
            ],
            &mut backend,
        )
        .unwrap();

        let (pane, paste) = &backend.sent[0];
        assert_eq!(*pane, 52, "right of %47 is %52");
        let text = String::from_utf8(paste.clone()).unwrap();
        assert!(text.starts_with("\x1b[200~[hitty ask — from the Codex agent in pane %47"));
        assert!(
            text.ends_with("\n\nReview the diff.\nBe brief.\x1b[201~"),
            "the message arrives whole, inside one bracket: {text:?}"
        );
        assert_eq!(
            backend.sent[1],
            (52, b"\r".to_vec()),
            "Enter is its own write, after the paste has landed"
        );
        assert_eq!(backend.sent.len(), 2);
        assert!(
            backend.captured.is_empty(),
            "an answer the agent reported is not read off the screen"
        );

        let (code, json) = json_of(outcome);
        assert_eq!(code, 0);
        assert_eq!(json["pane"], 52);
        assert_eq!(json["agent"], "claude");
        assert_eq!(json["answered"], true);
        assert_eq!(json["reply"], "Two issues:\n1. off by one");
        assert_eq!(json["reply_source"], "agent");
        assert_eq!(json["session_id"], "sess-52");
    }

    /// The reason the turn count exists: an answer quicker than one poll leaves
    /// the status exactly where it was.
    #[test]
    fn a_turn_that_finished_between_two_polls_is_still_the_answer() {
        let mut backend = backend();
        script(
            &mut backend,
            vec![
                vec![claude(AgentStatus::Done, 3, Some("the previous answer"))],
                vec![claude(AgentStatus::Done, 4, Some("42"))],
            ],
        );
        let (code, json) =
            json_of(run(&["ask", "%52", "6 × 7?", "--interval", "50"], &mut backend).unwrap());
        assert_eq!(code, 0);
        assert_eq!(json["reply"], "42");
    }

    #[test]
    fn a_standing_done_is_never_mistaken_for_the_answer() {
        let mut backend = backend();
        // Nothing moves: the message was typed, the agent has not taken it.
        script(
            &mut backend,
            vec![
                vec![claude(AgentStatus::Done, 3, Some("the previous answer"))],
                vec![claude(AgentStatus::Done, 3, Some("the previous answer"))],
            ],
        );
        let (code, json) = json_of(
            run(
                &[
                    "ask",
                    "claude",
                    "hello",
                    "--interval",
                    "50",
                    "--timeout",
                    "0",
                ],
                &mut backend,
            )
            .unwrap(),
        );
        assert_eq!(code, 124, "not yet, rather than last turn's reply");
        assert_eq!(json["answered"], false);
        assert_eq!(json["timed_out"], true);
        assert!(json.get("reply").is_none());
    }

    #[test]
    fn an_agent_that_reports_no_text_is_answered_from_its_screen_and_says_so() {
        let mut backend = backend();
        backend.capture_segments = vec![CaptureSegment {
            size: WinSize {
                cols: 40,
                rows: 5,
                cell_w: 8,
                cell_h: 16,
            },
            bytes: b"looks fine to me\r\n".to_vec(),
        }];
        script(
            &mut backend,
            vec![
                vec![claude(AgentStatus::Idle, 0, None)],
                vec![claude(AgentStatus::Working, 0, None)],
                // A server from before the turn count: only the level moves.
                vec![claude(AgentStatus::Done, 0, None)],
            ],
        );
        let (code, json) =
            json_of(run(&["ask", "right", "ok?", "--interval", "50"], &mut backend).unwrap());
        assert_eq!(code, 0);
        assert_eq!(json["reply"], "looks fine to me");
        assert_eq!(json["reply_source"], "screen");
        assert_eq!(backend.captured, vec![(52, false)]);
    }

    #[test]
    fn nothing_is_typed_into_an_agent_that_is_busy_or_waiting_on_its_user() {
        for (status, needle) in [
            (AgentStatus::Working, "in the middle of a turn"),
            (AgentStatus::Waiting, "waiting on its user"),
        ] {
            let mut backend = backend();
            script(&mut backend, vec![vec![claude(status, 1, None)]]);
            let err = run(&["ask", "right", "hello"], &mut backend)
                .unwrap_err()
                .to_string();
            assert!(err.contains(needle), "{err}");
            assert!(backend.sent.is_empty(), "{status:?}: nothing may be sent");
        }
    }

    #[test]
    fn ask_refuses_a_pane_with_no_agent_reporting_and_its_own_pane() {
        let mut backend = backend();
        script(&mut backend, vec![vec![]]);
        let err = run(&["ask", "right", "hello"], &mut backend)
            .unwrap_err()
            .to_string();
        assert!(err.contains("reports agent status"), "{err}");
        assert!(err.contains("hitty send"), "the other tool is named: {err}");

        let mut backend = backend_with_self_reporting();
        let err = run(&["ask", "%47", "hello"], &mut backend)
            .unwrap_err()
            .to_string();
        assert!(err.contains("is this pane"), "{err}");
        assert!(backend.sent.is_empty());
    }

    fn backend_with_self_reporting() -> MockBackend {
        let mut backend = backend();
        script(&mut backend, vec![vec![codex_caller()]]);
        backend
    }

    #[test]
    fn a_permission_prompt_raised_by_the_message_is_handed_to_the_user() {
        let mut backend = backend();
        let mut blocked = claude(AgentStatus::Waiting, 3, None);
        blocked.state.message = Some("Claude needs your permission to use Bash".into());
        script(
            &mut backend,
            vec![
                vec![claude(AgentStatus::Done, 3, None)],
                vec![claude(AgentStatus::Working, 3, None)],
                vec![blocked],
            ],
        );
        let (code, json) = json_of(
            run(
                &["ask", "right", "run it", "--interval", "50"],
                &mut backend,
            )
            .unwrap(),
        );
        assert_eq!(code, EXIT_NEEDS_USER);
        assert_eq!(json["status"], "waiting");
        assert_eq!(json["answered"], false);
        assert_eq!(json["message"], "Claude needs your permission to use Bash");
        assert_eq!(backend.sent.len(), 2, "the prompt is not answered for them");
    }

    #[test]
    fn an_agent_that_goes_away_ends_the_ask() {
        let mut backend = backend();
        script(
            &mut backend,
            vec![vec![claude(AgentStatus::Done, 3, None)], vec![]],
        );
        let (code, json) =
            json_of(run(&["ask", "right", "hello", "--interval", "50"], &mut backend).unwrap());
        assert_eq!(code, 1);
        assert_eq!(json["answered"], false);
    }

    #[test]
    fn no_wait_delivers_and_returns() {
        let mut backend = backend();
        script(&mut backend, vec![vec![claude(AgentStatus::Idle, 0, None)]]);
        let (code, json) =
            json_of(run(&["ask", "right", "start on it", "--no-wait"], &mut backend).unwrap());
        assert_eq!(code, 0);
        assert_eq!(json["answered"], false);
        assert_eq!(backend.sent.len(), 2);
    }

    #[test]
    fn a_message_cannot_close_its_own_paste_bracket() {
        let bytes = paste_bytes("a\x1b[201~\rrm -rf /\r\nb");
        let inner = &bytes[6..bytes.len() - 6];
        assert!(!inner.contains(&0x1b));
        assert!(!inner.contains(&b'\r'), "a bare CR would submit half of it");
    }

    #[test]
    fn peers_says_who_is_where_and_what_they_are_doing() {
        let mut backend = backend();
        script(
            &mut backend,
            vec![vec![codex_caller(), claude(AgentStatus::Done, 3, None)]],
        );
        let outcome = run(&["peers"], &mut backend).unwrap();
        let Outcome::Report(report) = outcome else {
            panic!("peers reports");
        };
        assert_eq!(report.json["pane"], 47);
        assert_eq!(
            report.json["peers"],
            json!([{
                "pane": 52, "direction": "right", "same_tab": true, "tab": 1,
                "agent": "claude", "status": "done", "title": null, "cwd": null,
            }])
        );
        let row = report.human.lines().nth(1).unwrap();
        assert_eq!(
            row.split_whitespace().collect::<Vec<_>>(),
            ["%52", "right", "claude", "done", "-"]
        );
    }
}

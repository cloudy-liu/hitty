//! An agent's status read off its screen, the way herdr reads it.
//!
//! The hooks only report what an agent chooses to announce, and some agents
//! announce nothing. Antigravity and Cursor have no hooks tty7 can install,
//! and Codex's hooks say nothing about a permission prompt. Herdr reads all of
//! them from the screen instead: each agent has a manifest of rules, a rule is
//! a set of text matches over one region of the live bottom of the pane, and
//! the highest-priority rule that matches names the state.
//!
//! The manifests in `assets/agent-detection/` are herdr's own, with tty7's
//! additions kept in `extensions/` (Apache-2.0; see the README there).
//! The matcher and region functions below follow herdr's
//! `src/detect/manifest.rs`, because a manifest
//! means only what its engine makes it mean.

use std::sync::OnceLock;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;
use regex::Regex;
use serde::Deserialize;

use crate::core::cli_agent::{AgentSessionState, AgentStatus, CLIAgent};

const MANIFEST_ENGINE_VERSION: u32 = 3;

/// What one look at the screen says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScreenState {
    Idle,
    Working,
    Blocked,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) state: ScreenState,
    /// Whether a rule matched. Without one this is herdr's fallback (idle for
    /// a known agent that shows nothing it recognises), which is a default and
    /// not evidence.
    pub(crate) matched: bool,
    /// The idle prompt itself is on screen, not just the absence of work.
    pub(crate) visible_idle: bool,
    /// The matched rule marks a screen that says nothing about the state (a
    /// transcript viewer, say), and the previous reading should stand.
    pub(crate) skip: bool,
}

pub(crate) struct Input<'a> {
    pub(crate) screen: &'a str,
    pub(crate) osc_title: &'a str,
    pub(crate) osc_progress: &'a str,
}

/// Classify a screen for `agent`. `None` when herdr has no manifest for it.
pub(crate) fn read(agent: CLIAgent, input: &Input) -> Option<Reading> {
    let manifest = compiled(agent)?;
    let mut best: Option<&CompiledRule> = None;
    for rule in &manifest.rules {
        if !rule.gate.matches(rule.region.of(input)) {
            continue;
        }
        // Earlier rules win ties, as in herdr.
        if best.is_none_or(|b| rule.priority > b.priority) {
            best = Some(rule);
        }
    }
    Some(match best {
        Some(rule) => Reading {
            state: rule.state,
            matched: true,
            visible_idle: rule.visible_idle && rule.state == ScreenState::Idle,
            skip: rule.skip_state_update,
        },
        None => Reading {
            // Codex's screen is too ambiguous to call idle by default.
            state: match agent {
                CLIAgent::Codex => ScreenState::Unknown,
                _ => ScreenState::Idle,
            },
            matched: false,
            visible_idle: false,
            skip: false,
        },
    })
}

fn manifest_source(agent: CLIAgent) -> Option<&'static str> {
    Some(match agent {
        CLIAgent::Claude => include_str!("../../assets/agent-detection/claude.toml"),
        CLIAgent::Codex => include_str!("../../assets/agent-detection/codex.toml"),
        CLIAgent::Gemini => include_str!("../../assets/agent-detection/gemini.toml"),
        CLIAgent::Amp => include_str!("../../assets/agent-detection/amp.toml"),
        CLIAgent::OpenCode => include_str!("../../assets/agent-detection/opencode.toml"),
        CLIAgent::Copilot => include_str!("../../assets/agent-detection/copilot.toml"),
        CLIAgent::Cursor => include_str!("../../assets/agent-detection/cursor.toml"),
        CLIAgent::Droid => include_str!("../../assets/agent-detection/droid.toml"),
        CLIAgent::Pi => include_str!("../../assets/agent-detection/pi.toml"),
        CLIAgent::Hermes => include_str!("../../assets/agent-detection/hermes.toml"),
        CLIAgent::Antigravity => include_str!("../../assets/agent-detection/antigravity.toml"),
        CLIAgent::Grok => include_str!("../../assets/agent-detection/grok.toml"),
        CLIAgent::Qwen => include_str!("../../assets/agent-detection/qwen.toml"),
        CLIAgent::Kimi => include_str!("../../assets/agent-detection/kimi.toml"),
        // Herdr reads the China build with the same manifest: its aliases list
        // `qoderclicn` and `qodercn` beside the global names.
        CLIAgent::QoderCLI | CLIAgent::QoderCLICn => {
            include_str!("../../assets/agent-detection/qodercli.toml")
        }
        CLIAgent::Aider
        | CLIAgent::Goose
        | CLIAgent::Auggie
        | CLIAgent::Vibe
        | CLIAgent::OhMyPi
        | CLIAgent::CodeBuddy => return None,
    })
}

/// The agents whose hooks herdr trusts over the screen once they report,
/// because they cover the whole turn: permission answers and interrupts
/// included. Herdr's list also has Kilo and Mastra Code, which tty7 does not
/// recognise.
fn hooks_are_authority(agent: CLIAgent) -> bool {
    matches!(
        agent,
        CLIAgent::Pi | CLIAgent::OhMyPi | CLIAgent::Kimi | CLIAgent::OpenCode
    )
}

/// What the screen says about the agent, once readings have been turned into a
/// turn's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenStatus {
    /// `None` is herdr's unknown: the screen is up, but nothing on it names a
    /// state. A Codex screen without a recognizable prompt/footer reads this way.
    pub(crate) status: Option<AgentStatus>,
    /// A rule backed the reading, rather than herdr's default for a screen
    /// with nothing on it the manifest recognises.
    pub(crate) evidenced: bool,
}

/// The status a pane shows, given what the hooks reported and what the screen
/// says.
///
/// Herdr rarely has to choose. Its hooks for Claude, Codex, Cursor and
/// Antigravity only name the session, so for those the screen alone says what
/// the agent is doing; only the agents in [`hooks_are_authority`] report a
/// state, and that state wins. tty7's hooks report a state for every agent
/// they are installed for, and they can miss an Esc or an approval, so outside
/// that list the screen keeps herdr's word whenever a rule backs it, and the
/// hooks fill in where none does.
pub(crate) fn merge(
    agent: CLIAgent,
    hook: Option<&AgentSessionState>,
    screen: Option<ScreenStatus>,
) -> Option<AgentStatus> {
    let hooked = hook.filter(|s| s.rich).map(|s| s.status);
    if hooked.is_some() && hooks_are_authority(agent) {
        return hooked;
    }
    match screen {
        // A turn short enough to start and end between two looks at the
        // screen still ended, and the hooks saw it.
        Some(ScreenStatus {
            status: Some(AgentStatus::Idle),
            ..
        }) if hooked == Some(AgentStatus::Done) => hooked,
        Some(ScreenStatus {
            status,
            evidenced: true,
        }) => status,
        Some(ScreenStatus {
            status,
            evidenced: false,
        }) => hooked.or(status),
        None => hook.map(|s| s.status),
    }
}

/// How many more looks a drop from working to a plain idle has to survive
/// before it counts, because an agent's screen goes quiet between the steps of
/// one turn. Herdr rechecks three times 100 ms apart; tty7 only looks every
/// 300 ms, so two more looks hold the drop for about 600 ms.
const IDLE_CONFIRMATIONS: u8 = 2;

/// Screen readings for one pane, turned into a turn's status: a reading of
/// idle after work becomes `Done`, which is what the hooks would have said.
#[derive(Default)]
pub(crate) struct Tracker {
    agent: Option<CLIAgent>,
    /// What the last reading looked at, so an unchanged screen is not read
    /// again.
    seen: Option<(u64, String, String)>,
    status: Option<ScreenStatus>,
    held_idle: u8,
}

impl Tracker {
    /// Take another look at the screen if it could have changed since the last
    /// one. `seq` moves whenever output arrives; `screen` renders the snapshot
    /// and is only called when a reading is due.
    pub(crate) fn observe(
        &mut self,
        agent: Option<CLIAgent>,
        seq: u64,
        title: &str,
        progress: &str,
        screen: impl FnOnce() -> String,
    ) {
        if agent != self.agent {
            *self = Tracker {
                agent,
                ..Tracker::default()
            };
        }
        let Some(agent) = agent.filter(|a| manifest_source(*a).is_some()) else {
            return;
        };
        let unchanged = self
            .seen
            .as_ref()
            .is_some_and(|(s, t, p)| *s == seq && t == title && p == progress);
        if unchanged && self.held_idle == 0 {
            return;
        }
        self.seen = Some((seq, title.to_string(), progress.to_string()));
        let screen = screen();
        let input = Input {
            screen: &screen,
            osc_title: title,
            osc_progress: progress,
        };
        if let Some(reading) = read(agent, &input) {
            self.apply(reading);
        }
    }

    fn apply(&mut self, reading: Reading) {
        if reading.skip {
            return;
        }
        let before = self.status.and_then(|s| s.status);
        // Only a plain idle is held: a visible prompt needs no second look,
        // and herdr shows an unknown screen at once.
        let plain_idle = reading.state == ScreenState::Idle && !reading.visible_idle;
        if before == Some(AgentStatus::Working) && plain_idle && self.held_idle < IDLE_CONFIRMATIONS
        {
            self.held_idle += 1;
            return;
        }
        self.held_idle = 0;
        let status = match reading.state {
            ScreenState::Working => Some(AgentStatus::Working),
            ScreenState::Blocked => Some(AgentStatus::Waiting),
            ScreenState::Idle => Some(match before {
                Some(AgentStatus::Working | AgentStatus::Waiting | AgentStatus::Done) => {
                    AgentStatus::Done
                }
                _ => AgentStatus::Idle,
            }),
            // Not a finished turn: herdr counts only idle after work as one.
            // Treating unknown as done had Codex, whose prompt reads unknown,
            // finish a turn every time it booted.
            ScreenState::Unknown => None,
        };
        self.status = Some(ScreenStatus {
            status,
            // A rule that says "unknown" (a model picker, a transcript
            // viewer) is no evidence of anything.
            evidenced: reading.matched && reading.state != ScreenState::Unknown,
        });
    }

    /// `None` until the screen has said anything, and for agents with no
    /// manifest.
    pub(crate) fn status(&self) -> Option<ScreenStatus> {
        self.status
    }
}

/// The live bottom of the pane as text, in herdr's detection shape: one line per
/// row, blank cells as spaces, trailing blank rows dropped. On the primary screen
/// the window ends at the lower of the cursor and the last row with text, so a
/// short UI at the top of a tall pane is still in view. Scrolling the viewport
/// back does not move it.
pub(crate) fn detection_text<T: EventListener>(term: &Term<T>) -> String {
    use alacritty_terminal::grid::Dimensions as _;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::TermMode;
    use alacritty_terminal::term::cell::Flags;

    let grid = term.grid();
    let rows = grid.screen_lines() as i32;
    let cols = grid.columns();
    if rows == 0 || cols == 0 {
        return String::new();
    }
    let row_text = |line: i32| -> String {
        let row = &grid[Line(line)];
        let mut text = String::with_capacity(cols);
        for col in 0..cols {
            let cell = &row[Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            text.push(cell.c);
            if let Some(zerowidth) = cell.zerowidth() {
                text.extend(zerowidth);
            }
        }
        text
    };
    let viewport: Vec<String> = (0..rows).map(row_text).collect();
    let end = match term.mode().contains(TermMode::ALT_SCREEN) {
        true => rows - 1,
        false => {
            let cursor = grid.cursor.point.line.0;
            viewport
                .iter()
                .rposition(|row| !row.trim().is_empty())
                .map_or(rows - 1, |last| (last as i32).max(cursor))
        }
    };
    let start = (end + 1 - rows).max(-(grid.history_size() as i32));
    let mut lines: Vec<String> = (start..=end)
        .map(|line| match line {
            0.. => viewport[line as usize].clone(),
            _ => row_text(line),
        })
        .collect();
    while lines.last().is_some_and(|row| row.trim().is_empty()) {
        lines.pop();
    }
    match lines.is_empty() {
        true => String::new(),
        false => lines.join("\n") + "\n",
    }
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    min_engine_version: Option<u32>,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct Rule {
    state: Option<ScreenState>,
    #[serde(default)]
    priority: i32,
    #[serde(default = "whole_recent")]
    region: String,
    #[serde(default)]
    visible_idle: bool,
    #[serde(default)]
    skip_state_update: bool,
    #[serde(flatten)]
    gate: Gate,
}

fn whole_recent() -> String {
    "whole_recent".to_string()
}

#[derive(Deserialize, Default)]
struct Gate {
    #[serde(default)]
    all: Vec<Gate>,
    #[serde(default)]
    any: Vec<Gate>,
    #[serde(default, rename = "not")]
    not_gate: Vec<Gate>,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    line_regex: Vec<String>,
}

struct CompiledManifest {
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    state: ScreenState,
    priority: i32,
    region: Region,
    visible_idle: bool,
    skip_state_update: bool,
    gate: CompiledGate,
}

/// Where a rule looks. Parsed when the manifest compiles, and an unknown name
/// fails it there, as herdr's validation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    OscTitle,
    /// The OSC 9;4 progress report, separate from the visible screen text.
    OscProgress,
    WholeRecent,
    AfterLastPromptMarker,
    BeforeCurrentPromptMarker,
    WholeRecentWithoutCurrentPromptMarker,
    CurrentPromptBlockMarker,
    AfterCurrentPromptBlockMarker,
    PromptBoxBody,
    AbovePromptBox,
    LastNonEmptyAbovePromptBox,
    AfterLastHorizontalRule,
    BottomLines(usize),
    BottomNonEmptyLines(usize),
    TopNonEmptyLines(usize),
}

impl Region {
    fn parse(spec: &str) -> Option<Region> {
        let spec = spec.trim();
        Some(match spec {
            "osc_title" => Region::OscTitle,
            "osc_progress" => Region::OscProgress,
            "whole_recent" => Region::WholeRecent,
            "after_last_prompt_marker" => Region::AfterLastPromptMarker,
            "before_current_prompt_marker" => Region::BeforeCurrentPromptMarker,
            "whole_recent_without_current_prompt_marker" => {
                Region::WholeRecentWithoutCurrentPromptMarker
            }
            "current_prompt_block_marker" => Region::CurrentPromptBlockMarker,
            "after_current_prompt_block_marker" => Region::AfterCurrentPromptBlockMarker,
            "prompt_box_body" => Region::PromptBoxBody,
            "above_prompt_box" => Region::AbovePromptBox,
            "last_non_empty_above_prompt_box" => Region::LastNonEmptyAbovePromptBox,
            "after_last_horizontal_rule" => Region::AfterLastHorizontalRule,
            _ => {
                if let Some(n) = region_count(spec, "bottom_lines") {
                    Region::BottomLines(n)
                } else if let Some(n) = region_count(spec, "bottom_non_empty_lines") {
                    Region::BottomNonEmptyLines(n)
                } else {
                    Region::TopNonEmptyLines(region_count(spec, "top_non_empty_lines")?)
                }
            }
        })
    }

    fn of<'a>(self, input: &Input<'a>) -> &'a str {
        let content = input.screen;
        match self {
            Region::OscTitle => input.osc_title,
            Region::OscProgress => input.osc_progress,
            Region::WholeRecent => content,
            Region::AfterLastPromptMarker => after_last_prompt_marker(content),
            Region::BeforeCurrentPromptMarker => before_current_prompt_marker(content),
            Region::WholeRecentWithoutCurrentPromptMarker => {
                match current_codex_prompt_index(&content.lines().collect::<Vec<_>>()) {
                    Some(_) => "",
                    None => content,
                }
            }
            Region::CurrentPromptBlockMarker => current_prompt_block_marker(content).unwrap_or(""),
            Region::AfterCurrentPromptBlockMarker => {
                after_current_prompt_block_marker(content).unwrap_or("")
            }
            Region::PromptBoxBody => prompt_box_body(content).unwrap_or(""),
            Region::AbovePromptBox => above_prompt_box(content),
            Region::LastNonEmptyAbovePromptBox => last_non_empty_line(above_prompt_box(content)),
            Region::AfterLastHorizontalRule => after_last_horizontal_rule(content),
            Region::BottomLines(n) => bottom_lines(content, n),
            Region::BottomNonEmptyLines(n) => bottom_non_empty_lines(content, n),
            Region::TopNonEmptyLines(n) => top_non_empty_lines(content, n),
        }
    }
}

struct CompiledGate {
    all: Vec<CompiledGate>,
    any: Vec<CompiledGate>,
    not_gate: Vec<CompiledGate>,
    /// Already lowercased: `contains` is case-insensitive.
    contains: Vec<String>,
    regex: Vec<Regex>,
    line_regex: Vec<Regex>,
}

impl CompiledGate {
    fn compile(gate: &Gate) -> Result<CompiledGate, regex::Error> {
        let gates = |list: &[Gate]| {
            list.iter()
                .map(CompiledGate::compile)
                .collect::<Result<Vec<_>, _>>()
        };
        let regexes = |list: &[String]| {
            list.iter()
                .map(|p| Regex::new(p))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(CompiledGate {
            all: gates(&gate.all)?,
            any: gates(&gate.any)?,
            not_gate: gates(&gate.not_gate)?,
            contains: gate.contains.iter().map(|c| c.to_lowercase()).collect(),
            regex: regexes(&gate.regex)?,
            line_regex: regexes(&gate.line_regex)?,
        })
    }

    fn matches(&self, text: &str) -> bool {
        self.matches_with(text, &text.to_lowercase())
    }

    fn matches_with(&self, text: &str, lower: &str) -> bool {
        self.contains
            .iter()
            .all(|needle| lower.contains(needle.as_str()))
            && self.regex.iter().all(|re| re.is_match(text))
            && self
                .line_regex
                .iter()
                .all(|re| text.lines().any(|line| re.is_match(line)))
            && self.all.iter().all(|g| g.matches_with(text, lower))
            && (self.any.is_empty() || self.any.iter().any(|g| g.matches_with(text, lower)))
            && !self.not_gate.iter().any(|g| g.matches_with(text, lower))
    }
}

fn compile(source: &str) -> Result<CompiledManifest, String> {
    let manifest: Manifest = toml::from_str(source).map_err(|e| e.to_string())?;
    if manifest.min_engine_version.unwrap_or(1) > MANIFEST_ENGINE_VERSION {
        return Err("manifest requires a newer detection engine".to_string());
    }
    let rules = manifest
        .rules
        .iter()
        .map(|rule| {
            Ok(CompiledRule {
                state: rule.state.unwrap_or(ScreenState::Unknown),
                priority: rule.priority,
                region: Region::parse(&rule.region)
                    .ok_or_else(|| format!("unknown region {}", rule.region))?,
                visible_idle: rule.visible_idle,
                skip_state_update: rule.skip_state_update,
                gate: CompiledGate::compile(&rule.gate).map_err(|e| e.to_string())?,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(CompiledManifest { rules })
}

fn compiled(agent: CLIAgent) -> Option<&'static CompiledManifest> {
    static CACHE: [OnceLock<Option<CompiledManifest>>; CLIAgent::ALL.len()] =
        [const { OnceLock::new() }; CLIAgent::ALL.len()];
    let slot = CLIAgent::ALL.iter().position(|a| *a == agent)?;
    CACHE[slot]
        .get_or_init(|| {
            let source = manifest_source(agent)?;
            let mut manifest = compile(source)
                .inspect_err(|e| log::warn!("{} screen manifest: {e}", agent.slug()))
                .ok()?;
            let extension_source = match agent {
                CLIAgent::Claude => Some(include_str!(
                    "../../assets/agent-detection/extensions/claude.toml"
                )),
                CLIAgent::Cursor => Some(include_str!(
                    "../../assets/agent-detection/extensions/cursor.toml"
                )),
                CLIAgent::Codex => Some(include_str!(
                    "../../assets/agent-detection/extensions/codex.toml"
                )),
                _ => None,
            };
            if let Some(source) = extension_source {
                let extension = compile(source)
                    .inspect_err(|e| log::warn!("{}'s local extension: {e}", agent.slug()))
                    .ok()?;
                manifest.rules.extend(extension.rules);
            }
            Some(manifest)
        })
        .as_ref()
}

fn region_count(spec: &str, name: &str) -> Option<usize> {
    spec.strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')?
        .parse()
        .ok()
}

fn bottom_lines(content: &str, count: usize) -> &str {
    let lines: Vec<&str> = content.lines().collect();
    from_line(content, &lines, lines.len().saturating_sub(count))
}

fn bottom_non_empty_lines(content: &str, count: usize) -> &str {
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, line)| !line.trim().is_empty())
        .take(count)
        .last()
        .map(|(i, _)| i);
    match start {
        Some(start) => from_line(content, &lines, start),
        None => "",
    }
}

fn top_non_empty_lines(content: &str, count: usize) -> &str {
    let lines: Vec<&str> = content.lines().collect();
    let end = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .take(count)
        .last()
        .map(|(i, _)| i);
    match end {
        Some(end) => &content[..line_start(content, &lines, end + 1)],
        None => "",
    }
}

fn after_last_prompt_marker(content: &str) -> &str {
    let lines: Vec<&str> = content.lines().collect();
    match lines.iter().rposition(|line| codex_prompt_line(line)) {
        Some(i) => from_line(content, &lines, i + 1),
        None => content,
    }
}

fn before_current_prompt_marker(content: &str) -> &str {
    let lines: Vec<&str> = content.lines().collect();
    match current_codex_prompt_index(&lines) {
        Some(mut i) => {
            // The input box's top border is not agent output. Leaving it at
            // the end of this region hides a live status line from Codex's
            // anchored working rule.
            while i > 0 && (lines[i - 1].trim().is_empty() || is_horizontal_rule(lines[i - 1])) {
                i -= 1;
            }
            &content[..line_start(content, &lines, i)]
        }
        None => content,
    }
}

fn current_prompt_block_marker(content: &str) -> Option<&str> {
    let lines: Vec<&str> = content.lines().collect();
    let prompt = current_codex_prompt_index(&lines)?;
    lines[..prompt]
        .iter()
        .rev()
        .find(|line| codex_block_marker_line(line))
        .copied()
}

fn after_current_prompt_block_marker(content: &str) -> Option<&str> {
    let lines: Vec<&str> = content.lines().collect();
    let prompt = current_codex_prompt_index(&lines)?;
    let block = lines[..prompt]
        .iter()
        .rposition(|line| codex_block_marker_line(line))?;
    Some(from_line(content, &lines, block))
}

fn current_codex_prompt_index(lines: &[&str]) -> Option<usize> {
    let prompt = lines.iter().rposition(|line| codex_prompt_line(line))?;
    match lines[prompt + 1..]
        .iter()
        .any(|line| codex_block_marker_line(line))
    {
        true => None,
        false => Some(prompt),
    }
}

fn codex_prompt_line(line: &str) -> bool {
    line == "›" || line.starts_with("› ")
}

fn codex_block_marker_line(line: &str) -> bool {
    line.starts_with(['•', '■', '✗', '✓'])
}

fn prompt_box_body(content: &str) -> Option<&str> {
    let lines: Vec<&str> = content.lines().collect();
    let top = prompt_box_top_border(&lines)?;
    let start = line_start(content, &lines, top + 1);
    let end = lines[top + 1..]
        .iter()
        .position(|line| is_horizontal_rule(line))
        .map_or(lines.len(), |i| top + 1 + i);
    Some(&content[start..line_start(content, &lines, end).max(start)])
}

fn above_prompt_box(content: &str) -> &str {
    let lines: Vec<&str> = content.lines().collect();
    match prompt_box_top_border(&lines) {
        Some(top) => &content[..line_start(content, &lines, top)],
        None => content,
    }
}

fn after_last_horizontal_rule(content: &str) -> &str {
    let mut rule_end = 0;
    let mut offset = 0;
    for line in content.lines() {
        offset += line.len() + 1;
        if is_horizontal_rule(line) {
            rule_end = offset.min(content.len());
        }
    }
    &content[rule_end..]
}

fn last_non_empty_line(content: &str) -> &str {
    content
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
}

/// The upper of the last two horizontal rules: the top of the input box.
fn prompt_box_top_border(lines: &[&str]) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, line)| is_horizontal_rule(line))
        .nth(1)
        .map(|(i, _)| i)
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    let rule = trimmed.chars().take_while(|&c| c == '─').count();
    if rule == 0 {
        return false;
    }
    let rest = trimmed[rule * '─'.len_utf8()..].trim_start();
    rest.is_empty() || rule >= 3
}

fn from_line<'a>(content: &'a str, lines: &[&str], index: usize) -> &'a str {
    &content[line_start(content, lines, index)..]
}

/// Byte offset of line `index`, counting one byte per line break the way
/// herdr's snapshot writes them.
fn line_start(content: &str, lines: &[&str], index: usize) -> usize {
    lines[..index.min(lines.len())]
        .iter()
        .map(|line| line.len() + 1)
        .sum::<usize>()
        .min(content.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(agent: CLIAgent, text: &str) -> Reading {
        read(
            agent,
            &Input {
                screen: text,
                osc_title: "",
                osc_progress: "",
            },
        )
        .expect("the agent has a manifest")
    }

    #[test]
    fn codex_159_visible_prompt_is_idle_and_finishes_work() {
        let prompt = "› Ask Codex to do anything\n\n  GPT-6-Astra xhigh · D:\\gh-prj\\tty7 · Context 4% used · Fast on\n  ← for agents · ? for shortcuts\n";
        let ready = screen(CLIAgent::Codex, prompt);
        assert_eq!(ready.state, ScreenState::Idle);
        assert!(ready.visible_idle);
        let mut tracker = Tracker::default();
        tracker.apply(screen(
            CLIAgent::Codex,
            "• Working (2s • esc to interrupt)\n› \n",
        ));
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Working));
        tracker.apply(screen(
            CLIAgent::Codex,
            &format!("• Result\n\n  Worked for 52s • 14:19\n\n{prompt}"),
        ));
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Done));
    }

    #[test]
    fn codex_ready_footer_does_not_override_work_or_approval() {
        let prompt = "›\n  ? for shortcuts\n";
        assert_eq!(screen(CLIAgent::Codex, prompt).state, ScreenState::Idle);
        assert_eq!(
            screen(
                CLIAgent::Codex,
                &format!("• Working (2s • esc to interrupt)\n{prompt}")
            )
            .state,
            ScreenState::Working
        );
        assert_eq!(
            screen(CLIAgent::Codex, &format!("{prompt}allow command?\n")).state,
            ScreenState::Blocked
        );
        assert_eq!(
            screen(
                CLIAgent::Codex,
                "› old prompt\n• Response still streaming\n  ? for shortcuts\n"
            )
            .state,
            ScreenState::Unknown
        );
        assert_eq!(
            screen(CLIAgent::Codex, "  ? for shortcuts\n").state,
            ScreenState::Unknown
        );
    }

    #[test]
    fn codex_static_title_marks_a_draft_idle() {
        // Codex keeps a static OSC title while a draft is being edited. Herdr
        // uses that title as idle evidence; the spinner and approval rules
        // above it still win when a turn is active.
        let draft = "previous response\n────────────────\n› dd\n────────────────\n  GPT-6.1-Sol xhigh · D:\\gh-prj\\ctty7 · Fast on\n";
        let reading = read(
            CLIAgent::Codex,
            &Input {
                screen: draft,
                osc_title: "OpenAI Codex (v0.162.1)",
                osc_progress: "",
            },
        )
        .expect("the agent has a manifest");
        assert_eq!(reading.state, ScreenState::Idle);
        assert!(reading.visible_idle);
    }

    #[test]
    fn codex_static_title_idle_yields_to_work_and_blocked_rules() {
        let title = "OpenAI Codex (v0.162.1)";
        assert_eq!(
            read(
                CLIAgent::Codex,
                &Input {
                    screen: "• Working (2s • esc to interrupt)\n› \n",
                    osc_title: title,
                    osc_progress: "",
                },
            )
            .unwrap()
            .state,
            ScreenState::Working
        );
        assert_eq!(
            read(
                CLIAgent::Codex,
                &Input {
                    screen: "›\nallow command?\n",
                    osc_title: title,
                    osc_progress: "",
                },
            )
            .unwrap()
            .state,
            ScreenState::Blocked
        );
    }

    #[test]
    fn codex_tool_timeout_stays_working_above_prompt_border() {
        let reading = read(
            CLIAgent::Codex,
            &Input {
                screen: "• Calling chrome-devtools.take_screenshot\n  + Show details\n\nWaiting for tool timeout (23m 54s • esc to interrupt)\n──────────────────\n› Ask Codex to do anything\n──────────────────\n  ? for shortcuts\n",
                osc_title: "OpenAI Codex (v0.162.1)",
                osc_progress: "",
            },
        )
        .unwrap();
        assert_eq!(reading.state, ScreenState::Working);
    }

    #[test]
    fn every_manifest_parses_and_compiles() {
        for agent in CLIAgent::ALL {
            if let Some(source) = manifest_source(agent) {
                let manifest =
                    compile(source).unwrap_or_else(|e| panic!("{}'s manifest: {e}", agent.slug()));
                assert!(!manifest.rules.is_empty(), "{} has no rules", agent.slug());
            }
        }
    }

    #[test]
    fn claude_named_session_question_is_blocked() {
        let text = concat!(
            "  4. Type something.\n",
            "  ──────────────────\n",
            "    5. Chat about this\n",
            "\n",
            "Enter to select · ↑/↓ to navigate · Esc to cancel\n",
            "──────────────────── probe named ─\n",
            "    3 tasks (1 done, 1 in progress, 1 open)\n",
            "    ✔ completed task\n",
            "    ◾ active task\n",
            "    ◻ open task\n",
            "    … +2 pending\n",
        );
        let reading = screen(CLIAgent::Claude, text);
        assert_eq!(reading.state, ScreenState::Blocked);
        assert!(reading.matched);
    }

    #[test]
    fn claude_named_session_question_survives_terminal_padding() {
        let text = include_str!("../../docs/testing/fixtures/claude-named-question.txt");
        let term = term_with_columns(32, 100, text.replace('\n', "\r\n").as_bytes());
        let reading = screen(CLIAgent::Claude, &detection_text(&term));
        assert_eq!(reading.state, ScreenState::Blocked);
        assert!(reading.matched);

        let newer = format!("{}\r\nNew activity\r\n", text.replace('\n', "\r\n"));
        let term = term_with_columns(32, 100, newer.as_bytes());
        assert_ne!(
            screen(CLIAgent::Claude, &detection_text(&term)).state,
            ScreenState::Blocked,
            "an old question must not override newer activity"
        );
    }

    #[test]
    fn a_rule_on_a_region_herdr_does_not_know_fails_the_manifest() {
        let source = "[[rules]]\nstate = \"idle\"\nregion = \"bottom_half\"\ncontains = [\"x\"]\n";
        assert!(compile(source).is_err());
        assert_eq!(
            Region::parse(" bottom_non_empty_lines(4) "),
            Some(Region::BottomNonEmptyLines(4))
        );
        assert_eq!(Region::parse("bottom_lines(x)"), None);
    }

    #[test]
    fn antigravity_reads_its_permission_prompt_and_its_spinner() {
        let blocked = screen(
            CLIAgent::Antigravity,
            "  Requesting permission for: run_command\n  rm -rf build\n  Do you want to proceed?\n  1. Yes\n",
        );
        assert_eq!(blocked.state, ScreenState::Blocked);
        assert!(blocked.matched);

        let working = screen(
            CLIAgent::Antigravity,
            "> fix the test\n ⠋ Thinking (esc to cancel)\n",
        );
        assert_eq!(working.state, ScreenState::Working);

        let idle = screen(CLIAgent::Antigravity, "> \n  ? for shortcuts\n");
        assert_eq!(idle.state, ScreenState::Idle);
        assert!(!idle.matched, "an idle agy is the fallback, not a rule");
    }

    #[test]
    fn antigravity_freeform_live_spinner_is_working() {
        let active = "○ Thought for 1m 50s\n● Explored 1 file\n● Killed 1 task\n⠿ The structure of the `view-grouped-container` section is being mapped out.\n└ Tip: Use /settings to configure your environment.\n> add emoji\n> \nPress up to edit queued messages\nGemini 3.8 Flash · high\n";
        let reading = screen(CLIAgent::Antigravity, active);
        assert_eq!(reading.state, ScreenState::Working);
        assert!(reading.matched);

        let mut tracker = Tracker::default();
        for seq in 1..=4 {
            tracker.observe(Some(CLIAgent::Antigravity), seq, "agy", "", || {
                active.into()
            });
            assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Working));
        }

        let finished = "○ Thought for 1m 50s\n● Explored 1 file\n● Killed 1 task\nThe structure of the `view-grouped-container` section has been mapped out.\n> \nGemini 3.8 Flash · high\n";
        for _ in 0..=IDLE_CONFIRMATIONS {
            tracker.observe(Some(CLIAgent::Antigravity), 5, "agy", "", || {
                finished.into()
            });
        }
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Done));
        assert_eq!(
            screen(CLIAgent::Antigravity, finished).state,
            ScreenState::Idle
        );
    }

    /// Qoder CLI 1.1.83 at rest: the shortcuts hint, the mode row and the
    /// composer. A turn adds a status line above them in Qoder's own wording
    /// (`status.model.thinking`, `status.loading.cancelAndTimer`).
    const QODER_COMPOSER: &str = "                                      ? for shortcuts\n\
        ──────────────────────────────────────────────────\n \
        YOLO Shift+Tab to Auto Mode          1 AGENTS.md file · 2 MCP servers\n\
        ──────────────────────────────────────────────────\n \
        *   Type your message or @path/to/file\n\
        ──────────────────────────────────────────────────\n";

    #[test]
    fn qoder_reads_its_turn_and_its_confirmation_prompt() {
        for agent in [CLIAgent::QoderCLI, CLIAgent::QoderCLICn] {
            let idle = screen(agent, QODER_COMPOSER);
            assert_eq!(idle.state, ScreenState::Idle, "{agent:?}");
            assert!(!idle.matched, "an idle Qoder is the fallback, not a rule");

            let working = screen(
                agent,
                &format!("⠙ Thinking... (esc to cancel, 4s)\n{QODER_COMPOSER}"),
            );
            assert_eq!(working.state, ScreenState::Working, "{agent:?}");

            let blocked = screen(
                agent,
                &format!(
                    " Allow this command to run?\n ❯ Allow once\n   Reject and type something\n\
                     ⠏ Waiting for user confirmation... (esc to cancel, 9s)\n{QODER_COMPOSER}"
                ),
            );
            assert_eq!(blocked.state, ScreenState::Blocked, "{agent:?}");
        }
    }

    #[test]
    fn a_qoder_turn_is_done_once_its_status_line_goes() {
        let working = format!("⠹ Generating... (esc to cancel, 2s)\n{QODER_COMPOSER}");
        let mut tracker = Tracker::default();
        tracker.observe(Some(CLIAgent::QoderCLI), 1, "", "", || working.clone());
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Working));
        for _ in 0..=IDLE_CONFIRMATIONS {
            tracker.observe(Some(CLIAgent::QoderCLI), 2, "", "", || {
                QODER_COMPOSER.into()
            });
        }
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Done));
    }

    #[test]
    fn upstream_rules_handle_current_prompts_and_background_work() {
        for (agent, text, expected) in [
            (CLIAgent::Pi, "⠋ Working\n", ScreenState::Working),
            (
                CLIAgent::Grok,
                "○ 2 commands still running\n> \n",
                ScreenState::Idle,
            ),
            (
                CLIAgent::Codex,
                "Update available · 0.159\nenter continue · esc skip\n",
                ScreenState::Blocked,
            ),
            (
                CLIAgent::Antigravity,
                "↑/↓ Navigate · tab Amend · f full diff\nesc to cancel\n",
                ScreenState::Blocked,
            ),
            (
                CLIAgent::Antigravity,
                "↑/↓ Navigate · enter Select · esc Skip\nesc to cancel\n",
                ScreenState::Blocked,
            ),
            (
                CLIAgent::Antigravity,
                "> \n? for shortcuts · 2 tasks\n",
                ScreenState::Idle,
            ),
        ] {
            assert_eq!(screen(agent, text).state, expected, "{agent:?}: {text}");
        }
    }

    #[test]
    fn manifests_requiring_a_newer_engine_are_rejected() {
        let future = "min_engine_version = 4\n[[rules]]\nstate = 'working'\ncontains = ['x']\n";
        assert!(
            compile(future).is_err(),
            "a future rule cannot silently use old semantics"
        );
    }

    #[test]
    fn progress_changes_are_observed_without_a_screen_or_title_change() {
        let mut tracker = Tracker::default();
        tracker.observe(Some(CLIAgent::Grok), 1, "", "4;1;-1", String::new);
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Working));
        tracker.observe(Some(CLIAgent::Grok), 1, "", "4;0;0", String::new);
        assert_eq!(tracker.status().unwrap().status, Some(AgentStatus::Done));
    }

    #[test]
    fn osc_progress_rules_receive_their_dedicated_input() {
        for (agent, progress, expected) in [
            (CLIAgent::Grok, "4;1;-1", ScreenState::Working),
            (CLIAgent::Qwen, "4;3;0", ScreenState::Working),
            (CLIAgent::Claude, "4;0;0", ScreenState::Idle),
        ] {
            let reading = read(
                agent,
                &Input {
                    screen: "",
                    osc_title: "",
                    osc_progress: progress,
                },
            )
            .unwrap();
            assert!(reading.matched, "{agent:?} must match its OSC rule");
            assert_eq!(reading.state, expected);
        }
    }

    #[test]
    fn cursor_reads_its_stop_hint_and_its_approval_prompt() {
        let working = screen(
            CLIAgent::Cursor,
            "  Reading src/main.rs\n\n  ⬢ Generating\n  ctrl+c to stop\n",
        );
        assert_eq!(working.state, ScreenState::Working);

        let blocked = screen(
            CLIAgent::Cursor,
            "  Run this command?\n  $ cargo test\n  Waiting for approval\n  Run (once) (y)\n  Skip (esc or n)\n",
        );
        assert_eq!(blocked.state, ScreenState::Blocked);
    }

    #[test]
    fn cursor_running_inside_herdr_reaches_the_sidebar_as_working() {
        // Reconstructed from Cursor 2026.10.14 inside Herdr: the outer terminal
        // includes Herdr's sidebar and divider before Cursor's live footer.
        let text = concat!(
            "  ctty7                    │ $ git branch -vv\n",
            "  working · cursor         │   322 output lines hidden · ctrl+o to expand\n",
            "                           │ ?? .release-verify/\n",
            "                           │ ?? .release-verify2/\n",
            "                           │\n",
            "                           │ ⠋ Running 591 tokens\n",
            "                           │ Tip: Hit shift+tab to enable Plan Mode for large or complex changes.\n",
            "                           │\n",
            "                           │ → Add a follow-up\n",
        );
        assert_eq!(
            screen(CLIAgent::Cursor, "⠋ Running 591 tokens\n").state,
            ScreenState::Working,
            "Cursor's standalone footer is recognized"
        );
        assert_eq!(
            screen(CLIAgent::Cursor, "│ ⠋ Running 591 tokens\n").state,
            ScreenState::Working,
            "a single Herdr divider reproduces the missed running state"
        );
        let mut tracker = Tracker::default();
        tracker.apply(screen(CLIAgent::Cursor, text));
        assert_eq!(tracker.status(), evidenced(AgentStatus::Working));
        assert_eq!(
            merge(
                CLIAgent::Cursor,
                Some(&session(AgentStatus::Idle, true)),
                tracker.status(),
            ),
            Some(AgentStatus::Working),
            "Cursor's running footer must reach the outer sidebar through Herdr's divider"
        );
    }

    #[test]
    fn cursor_nested_spinner_requires_a_pane_boundary_and_live_footer() {
        for prefix in ["", "  ", "│ ", "  ctty7 │ ", "  ctty7 │ shell │ "] {
            let reading = screen(CLIAgent::Cursor, &format!("{prefix}⠋ Running 591 tokens\n"));
            assert_eq!(reading.state, ScreenState::Working, "prefix: {prefix:?}");
            assert!(reading.matched);
        }
        for text in [
            "  working · cursor │ → Add a follow-up\n",
            "│ Running 591 tokens\n",
            "│ The previous status was ⠋ Running 591 tokens\n",
            "quoted ⠋ Running 591 tokens\n",
            "│ ⠋ Running 591 tokens\n│ one\n│ two\n│ three\n│ four\n│ five\n│ six\n│ seven\n│ → Add a follow-up\n",
        ] {
            assert_eq!(
                screen(CLIAgent::Cursor, text).state,
                ScreenState::Idle,
                "{text}"
            );
        }
    }

    #[test]
    fn cursor_nested_approval_takes_priority_over_a_running_spinner() {
        for prompt in ["Allow command (y)", "→ Run command (y)"] {
            let reading = screen(
                CLIAgent::Cursor,
                &format!("ctty7 │ ⠋ Running 591 tokens\n      │ {prompt}\n"),
            );
            assert_eq!(reading.state, ScreenState::Blocked, "{prompt}");
            assert!(reading.matched);
        }
    }

    #[test]
    fn codex_without_evidence_is_unknown_rather_than_idle() {
        let reading = screen(CLIAgent::Codex, "nothing recognisable\n");
        assert_eq!(reading.state, ScreenState::Unknown);
        assert!(!reading.matched);
    }

    #[test]
    fn claude_reads_its_title_spinner() {
        let reading = read(
            CLIAgent::Claude,
            &Input {
                screen: "",
                osc_title: "⠂ Fixing the build",
                osc_progress: "",
            },
        )
        .unwrap();
        assert_eq!(reading.state, ScreenState::Working);
        let reading = read(
            CLIAgent::Claude,
            &Input {
                screen: "",
                osc_title: "✳ Fixing the build",
                osc_progress: "",
            },
        )
        .unwrap();
        assert_eq!(reading.state, ScreenState::Idle);
        assert!(reading.visible_idle);
    }

    #[test]
    fn agents_herdr_has_no_manifest_for_read_nothing() {
        assert!(manifest_source(CLIAgent::Aider).is_none());
        let mut tracker = Tracker::default();
        tracker.observe(Some(CLIAgent::Aider), 1, "", "", || {
            panic!("no manifest, so nothing to render the screen for")
        });
        assert_eq!(tracker.status(), None);
    }

    fn reading(state: ScreenState, visible_idle: bool) -> Reading {
        Reading {
            state,
            matched: true,
            visible_idle,
            skip: false,
        }
    }

    /// An incomplete Codex prompt without the ready footer has no matching rule.
    const CODEX_PROMPT: Reading = Reading {
        state: ScreenState::Unknown,
        matched: false,
        visible_idle: false,
        skip: false,
    };

    fn evidenced(status: AgentStatus) -> Option<ScreenStatus> {
        Some(ScreenStatus {
            status: Some(status),
            evidenced: true,
        })
    }

    fn unbacked(status: Option<AgentStatus>) -> Option<ScreenStatus> {
        Some(ScreenStatus {
            status,
            evidenced: false,
        })
    }

    #[test]
    fn idle_after_work_is_a_finished_turn() {
        let mut t = Tracker::default();
        t.apply(reading(ScreenState::Idle, false));
        assert_eq!(t.status(), evidenced(AgentStatus::Idle), "nothing ran yet");
        t.apply(reading(ScreenState::Working, false));
        t.apply(reading(ScreenState::Blocked, false));
        assert_eq!(t.status(), evidenced(AgentStatus::Waiting));
        t.apply(reading(ScreenState::Idle, true));
        assert_eq!(t.status(), evidenced(AgentStatus::Done));
        t.apply(reading(ScreenState::Idle, true));
        assert_eq!(
            t.status(),
            evidenced(AgentStatus::Done),
            "done until the next turn"
        );
    }

    #[test]
    fn a_quiet_screen_mid_turn_must_repeat_before_it_ends_the_turn() {
        let mut t = Tracker::default();
        t.apply(reading(ScreenState::Working, false));
        for _ in 0..IDLE_CONFIRMATIONS {
            t.apply(reading(ScreenState::Idle, false));
            assert_eq!(t.status(), evidenced(AgentStatus::Working));
        }
        t.apply(reading(ScreenState::Idle, false));
        assert_eq!(t.status(), evidenced(AgentStatus::Done));

        let mut t = Tracker::default();
        t.apply(reading(ScreenState::Working, false));
        t.apply(reading(ScreenState::Idle, true));
        assert_eq!(
            t.status(),
            evidenced(AgentStatus::Done),
            "an idle prompt on screen needs no second look"
        );
    }

    #[test]
    fn an_unchanged_screen_is_not_read_twice() {
        let mut t = Tracker::default();
        let reads = std::cell::Cell::new(0);
        let screen = || {
            reads.set(reads.get() + 1);
            "> \n".to_string()
        };
        t.observe(Some(CLIAgent::Antigravity), 7, "agy", "", screen);
        t.observe(Some(CLIAgent::Antigravity), 7, "agy", "", screen);
        assert_eq!(reads.get(), 1);
        t.observe(Some(CLIAgent::Antigravity), 8, "agy", "", screen);
        assert_eq!(reads.get(), 2);
    }

    fn session(status: AgentStatus, rich: bool) -> AgentSessionState {
        AgentSessionState {
            status,
            rich,
            ..AgentSessionState::default()
        }
    }

    #[test]
    fn screen_evidence_outranks_hooks_that_can_miss_a_prompt() {
        let hook = session(AgentStatus::Working, true);
        assert_eq!(
            merge(
                CLIAgent::Codex,
                Some(&hook),
                evidenced(AgentStatus::Waiting)
            ),
            Some(AgentStatus::Waiting),
            "codex's hooks never report a permission prompt"
        );
        assert_eq!(
            merge(
                CLIAgent::Claude,
                Some(&hook),
                unbacked(Some(AgentStatus::Idle))
            ),
            Some(AgentStatus::Working),
            "a fallback idle is no reason to overrule a reporting hook"
        );
        assert_eq!(
            merge(CLIAgent::Kimi, Some(&hook), evidenced(AgentStatus::Done)),
            Some(AgentStatus::Working),
            "kimi's hooks cover the whole turn, as herdr trusts them to"
        );
        let done = session(AgentStatus::Done, true);
        assert_eq!(
            merge(CLIAgent::Claude, Some(&done), evidenced(AgentStatus::Idle)),
            Some(AgentStatus::Done),
            "a turn that ended between two looks at the screen still ended"
        );
    }

    #[test]
    fn codex_incomplete_prompt_remains_unknown() {
        // Codex boots behind a spinner in its title, then sits at a prompt that
        // lacks enough evidence for the ready-prompt rule.
        let mut t = Tracker::default();
        t.apply(reading(ScreenState::Working, false));
        t.apply(CODEX_PROMPT);
        assert_eq!(
            t.status(),
            unbacked(None),
            "the spinner is gone, and herdr shows the prompt as unknown at once"
        );
        assert_eq!(
            merge(CLIAgent::Codex, None, t.status()),
            None,
            "not done: a boot is no finished turn to notify or badge"
        );
        let hook = session(AgentStatus::Idle, true);
        assert_eq!(
            merge(CLIAgent::Codex, Some(&hook), t.status()),
            Some(AgentStatus::Idle),
            "a hook that has reported gets the call"
        );
    }

    #[test]
    fn only_idle_after_work_finishes_a_turn_not_unknown() {
        let mut t = Tracker::default();
        t.apply(reading(ScreenState::Working, false));
        t.apply(CODEX_PROMPT);
        t.apply(reading(ScreenState::Idle, true));
        assert_eq!(
            t.status(),
            evidenced(AgentStatus::Idle),
            "herdr marks a turn seen once it leaves working for unknown"
        );
    }

    #[test]
    fn a_screen_the_manifest_calls_unknown_defers_to_the_hooks() {
        // Claude's model picker is a rule whose state is "unknown".
        let mut t = Tracker::default();
        t.apply(reading(ScreenState::Idle, true));
        t.apply(reading(ScreenState::Unknown, false));
        assert_eq!(t.status(), unbacked(None));
        let hook = session(AgentStatus::Waiting, true);
        assert_eq!(
            merge(CLIAgent::Claude, Some(&hook), t.status()),
            Some(AgentStatus::Waiting)
        );
        assert_eq!(merge(CLIAgent::Claude, None, t.status()), None);
    }

    #[test]
    fn with_no_hooks_the_screen_is_all_there_is() {
        assert_eq!(
            merge(
                CLIAgent::Antigravity,
                None,
                unbacked(Some(AgentStatus::Idle))
            ),
            Some(AgentStatus::Idle)
        );
        assert_eq!(
            merge(CLIAgent::Antigravity, None, None),
            None,
            "not read yet"
        );
        let notified = session(AgentStatus::Waiting, false);
        assert_eq!(
            merge(CLIAgent::Aider, Some(&notified), None),
            Some(AgentStatus::Waiting),
            "a bare notification still counts where there is no screen reading"
        );
    }

    /// Herdr has no CodeBuddy manifest, so its status is what its hooks report.
    #[test]
    fn codebuddy_status_comes_from_its_hooks() {
        let mut tracker = Tracker::default();
        tracker.observe(Some(CLIAgent::CodeBuddy), 1, "", "", || {
            unreachable!("there is no manifest to read the screen with")
        });
        assert_eq!(tracker.status(), None);
        let hook = session(AgentStatus::Waiting, true);
        assert_eq!(
            merge(CLIAgent::CodeBuddy, Some(&hook), tracker.status()),
            Some(AgentStatus::Waiting)
        );
    }

    #[test]
    fn regions_follow_herdr() {
        let text = "a\n\nb\nc\n\n";
        assert_eq!(bottom_non_empty_lines(text, 2), "b\nc\n\n");
        assert_eq!(bottom_lines(text, 2), "c\n\n");
        assert_eq!(top_non_empty_lines(text, 2), "a\n\nb\n");
        let boxed = "out\n────────\n> type here\n────────\nhint\n";
        assert_eq!(prompt_box_body(boxed), Some("> type here\n"));
        assert_eq!(above_prompt_box(boxed), "out\n");
        assert_eq!(after_last_horizontal_rule(boxed), "hint\n");
        let codex = "• Ran tests\n› \n";
        assert_eq!(current_prompt_block_marker(codex), Some("• Ran tests"));
        assert_eq!(before_current_prompt_marker(codex), "• Ran tests\n");
    }

    fn term_with(rows: usize, bytes: &[u8]) -> Term<alacritty_terminal::event::VoidListener> {
        term_with_columns(rows, 20, bytes)
    }

    fn term_with_columns(
        rows: usize,
        columns: usize,
        bytes: &[u8],
    ) -> Term<alacritty_terminal::event::VoidListener> {
        use alacritty_terminal::term::Config;
        use alacritty_terminal::vte::ansi::Processor;
        struct Size(usize, usize);
        impl alacritty_terminal::grid::Dimensions for Size {
            fn total_lines(&self) -> usize {
                self.0
            }
            fn screen_lines(&self) -> usize {
                self.0
            }
            fn columns(&self) -> usize {
                self.1
            }
        }
        let mut term = Term::new(
            Config::default(),
            &Size(rows, columns),
            alacritty_terminal::event::VoidListener,
        );
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        term
    }

    #[test]
    fn detection_text_ends_at_the_live_bottom() {
        let term = term_with(4, b"one\r\ntwo\r\n");
        assert_eq!(
            detection_text(&term),
            format!("{:20}\n{:20}\n", "one", "two"),
            "rows keep their width, and the empty rows under the cursor go"
        );

        let term = term_with(3, b"1\r\n2\r\n3\r\n4\r\n5");
        let text = detection_text(&term);
        assert!(text.contains('5') && text.contains('3'));
        assert!(!text.contains('2'), "only a screenful: {text:?}");
    }

    #[test]
    fn detection_text_skips_the_spacer_after_a_wide_glyph() {
        let term = term_with(2, "界x".as_bytes());
        assert!(detection_text(&term).starts_with("界x"));
    }
}

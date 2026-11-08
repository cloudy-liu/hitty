//! Recorded shell-integration command boundaries; navigation never writes PTY input.
use super::agent_marks::{AgentTurnScanner, AgentTurns, TurnCut};
use crate::core::osc::OscTokenizer;
use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    term::{Term, TermMode},
    vte::ansi::Processor,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandEntry {
    pub id: u64,
    pub text: String,
    pub row: Option<i64>,
    pub exit: Option<i32>,
    pub done: bool,
}

#[derive(Default)]
struct Inner {
    entries: Vec<CommandEntry>,
    next: u64,
    prompt: Option<i64>,
    geometry: Option<(usize, usize)>,
    history: usize,
}

#[derive(Clone)]
pub(crate) struct CommandMarks {
    inner: Arc<Mutex<Inner>>,
    limit: usize,
}

impl CommandMarks {
    pub fn new(limit: usize) -> Self {
        Self {
            inner: Default::default(),
            limit,
        }
    }
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.clear();
            inner.prompt = None;
        }
    }
    fn validate<T: EventListener>(&self, term: &Term<T>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let geometry = (term.columns(), term.screen_lines());
        let history = term.grid().history_size();
        // The emulator does not expose a discarded-row counter. Once history
        // fills, conservatively retire anchors rather than guess at shifted rows.
        if history >= self.limit
            || history < inner.history
            || inner.geometry.is_some_and(|old| old != geometry)
        {
            for entry in &mut inner.entries {
                entry.row = None;
            }
            inner.prompt = None;
        }
        inner.geometry = Some(geometry);
        inner.history = history;
    }
    pub fn list<T: EventListener>(&self, term: &Term<T>) -> Vec<CommandEntry> {
        self.validate(term);
        let Ok(inner) = self.inner.lock() else {
            return Vec::new();
        };
        let mut entries = inner.entries.clone();
        if term.mode().contains(TermMode::ALT_SCREEN) {
            for entry in &mut entries {
                entry.row = None;
            }
        }
        entries
    }
    fn apply<T: EventListener>(&self, term: &Term<T>, cut: CommandCut) {
        self.validate(term);
        let row = (!term.mode().contains(TermMode::ALT_SCREEN)
            && term.grid().history_size() < self.limit)
            .then(|| {
                term.grid().history_size() as i64 + i64::from(term.grid().cursor.point.line.0)
            });
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        match cut {
            CommandCut::Prompt => {
                inner.prompt = row;
            }
            CommandCut::Begin(raw) => {
                if let Some(last) = inner.entries.last_mut() {
                    last.done = true;
                }
                let text = super::view::unescape_mark_text(&raw);
                if text.trim().is_empty() {
                    return;
                }
                let id = inner.next;
                inner.next += 1;
                let row = inner.prompt.take().or(row);
                inner.entries.push(CommandEntry {
                    id,
                    text: text.chars().take(512).collect(),
                    row,
                    exit: None,
                    done: false,
                });
                if inner.entries.len() > 500 {
                    inner.entries.remove(0);
                }
            }
            CommandCut::End(exit) => {
                if let Some(last) = inner.entries.last_mut().filter(|e| !e.done) {
                    last.exit = exit;
                    last.done = true;
                }
            }
        }
    }
}

enum CommandCut {
    Prompt,
    Begin(String),
    End(Option<i32>),
}
enum Cut {
    Agent(TurnCut),
    Command(CommandCut),
}

/// Both navigation providers observe the same stream cuts before subsequent
/// output moves the cursor. No second emulator or shell history is needed.
pub(crate) struct OutputOutline {
    agent_scan: AgentTurnScanner,
    command_scan: OscTokenizer,
    turns: AgentTurns,
    commands: CommandMarks,
}

impl OutputOutline {
    pub fn new(turns: AgentTurns, commands: CommandMarks) -> Self {
        Self {
            agent_scan: AgentTurnScanner::new(),
            command_scan: OscTokenizer::new(&[b"133"]),
            turns,
            commands,
        }
    }
    pub fn reset(&mut self) {
        self.agent_scan.reset();
        self.command_scan = OscTokenizer::new(&[b"133"]);
        self.commands.clear();
    }
    pub fn advance<T: EventListener>(
        &mut self,
        processor: &mut Processor,
        term: &mut Term<T>,
        bytes: &[u8],
    ) {
        let mut cuts = Vec::new();
        self.agent_scan
            .feed(bytes, |at, cut| cuts.push((at, Cut::Agent(cut))));
        self.command_scan.feed_at(bytes, |at, payload| {
            let Some(rest) = payload.strip_prefix(b"133;") else {
                return;
            };
            let cut = match rest
                .splitn(2, |&c| c == b';')
                .collect::<Vec<_>>()
                .as_slice()
            {
                [b"A", ..] => Some(CommandCut::Prompt),
                [b"C", text] => Some(CommandCut::Begin(
                    String::from_utf8_lossy(text).into_owned(),
                )),
                [b"C"] => Some(CommandCut::Begin(String::new())),
                [b"D", exit] => Some(CommandCut::End(
                    std::str::from_utf8(exit).ok().and_then(|s| s.parse().ok()),
                )),
                [b"D"] => Some(CommandCut::End(None)),
                _ => None,
            };
            if let Some(cut) = cut {
                cuts.push((at, Cut::Command(cut)));
            }
        });
        cuts.sort_by_key(|(at, _)| *at);
        let mut last = 0;
        for (at, cut) in cuts {
            processor.advance(term, &bytes[last..at]);
            last = at;
            match cut {
                Cut::Agent(cut) => self.turns.apply(term, cut),
                Cut::Command(cut) => self.commands.apply(term, cut),
            }
        }
        processor.advance(term, &bytes[last..]);
        self.commands.validate(term);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TermSize;
    use alacritty_terminal::{event::VoidListener, term::Config};

    #[test]
    fn command_entries_retire_when_scrollback_or_geometry_cannot_preserve_anchors() {
        let mut term = Term::new(
            Config {
                scrolling_history: 3,
                ..Default::default()
            },
            &TermSize::new(40, 3),
            VoidListener,
        );
        let commands = CommandMarks::new(3);
        let mut output = OutputOutline::new(AgentTurns::new(), commands.clone());
        let mut processor = Processor::new();
        output.advance(
            &mut processor,
            &mut term,
            b"\x1b]133;A\x07$ test\r\n\x1b]133;C;test\x07ok\r\n\x1b]133;D;0\x07",
        );
        assert_eq!(commands.list(&term)[0].row, Some(0));
        output.advance(
            &mut processor,
            &mut term,
            b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n",
        );
        assert!(commands.list(&term)[0].row.is_none());
        output.reset();
        assert!(commands.list(&term).is_empty());
        output.advance(
            &mut processor,
            &mut term,
            b"\x1b[?1049h\x1b]133;A\x07\x1b]133;C;full-screen\x07",
        );
        assert!(commands.list(&term)[0].row.is_none());
    }
}

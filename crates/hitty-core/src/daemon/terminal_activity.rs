use std::collections::HashMap;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

use super::protocol::WinSize;

/// Read the same terminal cells the user sees. An agent may address a row for
/// both a new response and a repaint, so cursor movement alone says very little.
pub(super) struct OutputActivity {
    term: Term<VoidListener>,
    parser: Processor,
    previous: HashMap<String, usize>,
}

impl OutputActivity {
    pub(super) fn new(size: WinSize) -> Self {
        let config = Config {
            scrolling_history: 32,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &ActivitySize::from(size), VoidListener),
            parser: Processor::new(),
            previous: HashMap::new(),
        }
    }

    pub(super) fn feed(&mut self, bytes: &[u8], size: WinSize) -> bool {
        let size = ActivitySize::from(size);
        if self.term.columns() != size.columns() || self.term.screen_lines() != size.screen_lines()
        {
            self.term.resize(size);
            self.previous = self.content();
        }
        self.parser.advance(&mut self.term, bytes);
        let current = self.content();
        let changed = current
            .iter()
            .any(|(line, count)| *count > self.previous.get(line).copied().unwrap_or_default());
        self.previous = current;
        changed
    }

    fn content(&self) -> HashMap<String, usize> {
        let start = Point::new(self.term.topmost_line(), Column(0));
        let end = Point::new(self.term.bottommost_line(), self.term.last_column());
        let text = self.term.bounds_to_string(start, end);
        let mut content = HashMap::new();
        for line in text.lines().filter_map(conversation_line) {
            *content.entry(line.to_owned()).or_default() += 1;
        }
        content
    }
}

fn conversation_line(line: &str) -> Option<&str> {
    let line = line.trim();
    // Codex and other terminal agents repaint their timers and spinner glyphs
    // in place. The text elsewhere in a redraw is compared with the last frame.
    let line = line
        .trim_start_matches(|ch| {
            matches!(
                ch,
                '\u{2800}'
                    ..='\u{28ff}' | '\u{2736}' | '\u{273b}' | '\u{273d}' | '\u{2733}' | '\u{273f}'
            )
        })
        .trim_start();
    let status = line
        .trim_start_matches(['\u{2022}', '\u{25e6}'])
        .trim_start();
    if status == "Working" || status.starts_with("Working (") {
        return None;
    }
    let elapsed = line.rsplit_once(" (").filter(|(_, tail)| {
        let Some((duration, _)) = tail.split_once(')') else {
            return false;
        };
        let duration = duration.split(" \u{2022} ").next().unwrap_or_default();
        !duration.is_empty()
            && duration.split_whitespace().all(|part| {
                part.strip_suffix(['h', 'm', 's']).is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                })
            })
    });
    let content = elapsed.map_or(line, |(label, _)| label);
    (!content.is_empty() && !content.eq_ignore_ascii_case("esc to interrupt")).then_some(content)
}

/// Bound the supplemental activity grid, including sizes reported by a CLI.
struct ActivitySize {
    columns: usize,
    lines: usize,
}

impl From<WinSize> for ActivitySize {
    fn from(size: WinSize) -> Self {
        Self {
            columns: usize::from(size.cols).clamp(2, 512),
            lines: usize::from(size.rows).clamp(2, 256),
        }
    }
}

impl Dimensions for ActivitySize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: WinSize = WinSize {
        cols: 80,
        rows: 24,
        cell_w: 9,
        cell_h: 17,
    };

    #[test]
    fn positioned_text_counts_but_multiline_spinner_repaints_stay_quiet() {
        let mut output = OutputActivity::new(SIZE);
        assert!(!output.feed(b"\x1b[1;1HWorking (4s)\r\nEsc to interrupt", SIZE));
        assert!(!output.feed(b"\x1b[1;1HWorking (5s)\r\nEsc to interrupt", SIZE));
        assert!(!output.feed(b"\x1b[1;1H\x1b[KWorking (6", SIZE));
        assert!(!output.feed(b"s)", SIZE));
        assert!(output.feed(b"\x1b[6;1H\x1b[KNew agent response", SIZE));
        assert!(!output.feed(b"\x1b[6;1H\x1b[KNew agent response\x1b[?25h", SIZE));
        assert!(output.feed(b" continued", SIZE));
        assert!(output.feed(b"\x1b[7;1HReading files (4s)", SIZE));
        assert!(!output.feed(b"\x1b[7;1HReading files (5s)", SIZE));
        assert!(!output.feed(b"\x1b]0;another title\x07", SIZE));
        assert!(!output.feed(b"", WinSize { cols: 70, ..SIZE }));
    }
}

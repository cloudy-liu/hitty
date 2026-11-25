//! Detect forward text from the live PTY without treating in-place repaints
//! such as a spinner or a cursor blink as new conversation output.

#[derive(Default)]
pub(super) struct OutputActivity {
    parser: vte::Parser,
    text: ForwardText,
}

impl OutputActivity {
    pub(super) fn feed(&mut self, bytes: &[u8]) -> bool {
        self.parser.advance(&mut self.text, bytes);
        std::mem::take(&mut self.text.changed)
    }
}

#[derive(Default)]
struct ForwardText {
    rewriting: bool,
    changed: bool,
}

impl vte::Perform for ForwardText {
    fn print(&mut self, ch: char) {
        self.changed |= !self.rewriting && !ch.is_whitespace();
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' | 0x08 => self.rewriting = true,
            b'\n' | 0x0b | 0x0c => self.rewriting = false,
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, _: &vte::Params, intermediate: &[u8], ignore: bool, action: char) {
        if !ignore
            && intermediate.is_empty()
            && matches!(
                action,
                'A' | 'D' | 'F' | 'G' | 'H' | 'J' | 'K' | 'd' | 'f' | 'u'
            )
        {
            self.rewriting = true;
        }
    }

    fn esc_dispatch(&mut self, intermediate: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediate.is_empty() {
            match byte {
                b'D' | b'E' => self.rewriting = false,
                b'M' | b'8' => self.rewriting = true,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_text_counts_and_in_place_repaints_stay_quiet() {
        let mut output = OutputActivity::default();
        assert!(output.feed(b"Reading "));
        assert!(output.feed(b"src/main.rs"));
        assert!(!output.feed(b"\r\x1b[2KWorking (4s)"));
        assert!(!output.feed(b"\r\x1b[2KWorking (5s)\x1b[?25h"));
        assert!(!output.feed(b"\x1b]0;another title"));
        assert!(!output.feed(b"\x07\x1b[3;1HWorking (6s)"));
        assert!(output.feed(b"\r\n\x1b[32mResult\x1b[0m"));
    }
}

//! ConPTY's startup query, answered where the pty lives.
//!
//! The first thing a pseudoconsole does when its shell connects is ask the
//! terminal what it is — `ESC [ c`, primary device attributes — and it holds
//! the shell back until an answer arrives, giving up only after three seconds
//! (`VtIo::StartIfNeeded` calling `WaitUntilDA1(3000)`).
//!
//! The only emulator is in the window, and it answers `ESC [ ? 6 c`: a VT102.
//! The bundled ConPTY goes on waiting through any answer below VT220, so every
//! pane it hosted started three seconds late, however promptly its client
//! replied. A client that was late made no difference, and neither did a query
//! that reached the replay ring first and came back as history.
//!
//! The daemon parses nothing else it carries. This one query is different in
//! kind: it is addressed to whoever opened the pseudoconsole, it arrives before
//! the shell has run a single instruction, and its answer is a constant. So the
//! pane's reader answers it and lifts it out of the stream. The shell starts
//! without waiting for a client to attach, build a view and reply, and no
//! client is left a query to answer again.

use std::borrow::Cow;

/// Primary device attributes, as ConPTY asks for them.
const QUERY: &[u8] = b"\x1b[c";

/// A VT220 with no optional features.
///
/// The conformance level is what ends ConPTY's wait: measured against the
/// bundled 1.24, `?62;c`, `?62c` and `?62;1;2;6;7;8;9c` are taken at once,
/// while `?6c`, `?1;2c` and `?c` are waited through like silence. The features
/// are all ConPTY keeps, and it consults them for one thing — whether it may
/// scroll with rectangular-area operations — so claiming none leaves it
/// exactly where the timeout used to: with the same assumptions, three seconds
/// sooner.
pub(crate) const REPLY: &[u8] = b"\x1b[?62;c";

/// How far into a pane's output the query can start. ConPTY writes its
/// handshake — `ESC[1t ESC[c ESC[?1004h ESC[?9001h`, a couple of dozen bytes —
/// before the shell is allowed to run, so a query further in was written by a
/// program and is the emulator's to answer.
const WINDOW: usize = 256;

pub(crate) struct StartupQuery {
    /// Output still to be searched. Zero once the query has been answered or
    /// the pane has written past [`WINDOW`], and from the start for a pane
    /// that has no startup query coming.
    window: usize,
    /// The end of the last read, when it could be the start of the query. An
    /// unfinished escape sequence means nothing to a terminal until the rest
    /// of it arrives, so keeping it for one read changes nothing on screen.
    held: Vec<u8>,
}

impl StartupQuery {
    /// `fresh_conpty` says the pseudoconsole was opened for this pane just
    /// now. Any other pane — adopted, hosted elsewhere, not a ConPTY at all —
    /// has no startup query coming, and an `ESC [ c` in its output is a
    /// program's.
    pub(crate) fn new(fresh_conpty: bool) -> Self {
        Self {
            window: if fresh_conpty { WINDOW } else { 0 },
            held: Vec::new(),
        }
    }

    /// One pty read without the startup query, and whether the query was in
    /// it — in which case the caller owes ConPTY a [`REPLY`].
    pub(crate) fn strip<'a>(&mut self, bytes: &'a [u8]) -> (Cow<'a, [u8]>, bool) {
        if self.window == 0 {
            return (Cow::Borrowed(bytes), false);
        }
        let mut data = if self.held.is_empty() {
            Cow::Borrowed(bytes)
        } else {
            let mut joined = std::mem::take(&mut self.held);
            joined.extend_from_slice(bytes);
            Cow::Owned(joined)
        };

        // Only a query that starts inside the window is ConPTY's.
        let searched = data.len().min(self.window + QUERY.len() - 1);
        if let Some(at) = data[..searched]
            .windows(QUERY.len())
            .position(|window| window == QUERY)
        {
            self.window = 0;
            let mut rest = data.into_owned();
            rest.drain(at..at + QUERY.len());
            return (Cow::Owned(rest), true);
        }

        let unfinished = unfinished_query(&data);
        let passed = data.len() - unfinished;
        self.window = self.window.saturating_sub(passed);
        if self.window == 0 || unfinished == 0 {
            return (data, false);
        }
        self.held = data[passed..].to_vec();
        match &mut data {
            Cow::Borrowed(bytes) => *bytes = &bytes[..passed],
            Cow::Owned(bytes) => bytes.truncate(passed),
        }
        (data, false)
    }
}

/// How many bytes at the end of `data` are the start of a [`QUERY`] that the
/// next read could finish.
fn unfinished_query(data: &[u8]) -> usize {
    (1..QUERY.len())
        .rev()
        .find(|&len| data.ends_with(&QUERY[..len]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a fresh ConPTY writes before the shell has run.
    const HANDSHAKE: &[u8] = b"\x1b[1t\x1b[c\x1b[?1004h\x1b[?9001h";
    const HANDSHAKE_WITHOUT_QUERY: &[u8] = b"\x1b[1t\x1b[?1004h\x1b[?9001h";

    /// Everything the reads let through, and how many replies they called for.
    fn pass(query: &mut StartupQuery, reads: &[&[u8]]) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        let mut replies = 0;
        for read in reads {
            let (bytes, asked) = query.strip(read);
            out.extend_from_slice(&bytes);
            replies += usize::from(asked);
        }
        (out, replies)
    }

    #[test]
    fn the_startup_query_is_answered_and_the_rest_of_the_handshake_passes() {
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[HANDSHAKE, b"C:\\work>"]);
        assert_eq!(replies, 1);
        // Focus reporting and win32 input mode are the client's to act on.
        assert_eq!(out, [HANDSHAKE_WITHOUT_QUERY, b"C:\\work>"].concat());
    }

    #[test]
    fn a_handshake_split_anywhere_across_reads_loses_only_the_query() {
        for split in 0..=HANDSHAKE.len() {
            let (head, tail) = HANDSHAKE.split_at(split);
            let mut query = StartupQuery::new(true);
            let (out, replies) = pass(&mut query, &[head, tail]);
            assert_eq!(replies, 1, "split at {split}");
            assert_eq!(out, HANDSHAKE_WITHOUT_QUERY, "split at {split}");
        }

        let single_bytes: Vec<&[u8]> = HANDSHAKE.chunks(1).collect();
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &single_bytes);
        assert_eq!(replies, 1);
        assert_eq!(out, HANDSHAKE_WITHOUT_QUERY);
    }

    #[test]
    fn no_part_of_a_split_query_reaches_the_stream_early() {
        let mut query = StartupQuery::new(true);
        let (first, asked) = query.strip(b"\x1b[1t\x1b[");
        assert_eq!((&*first, asked), (&b"\x1b[1t"[..], false));
        let (second, asked) = query.strip(b"c\x1b[?1004h");
        assert_eq!((&*second, asked), (&b"\x1b[?1004h"[..], true));
    }

    #[test]
    fn a_held_escape_that_is_not_the_query_passes_intact() {
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[b"\x1b[1t\x1b", b"[?1004h\x1b[", b"?9001h"]);
        assert_eq!(replies, 0);
        assert_eq!(out, HANDSHAKE_WITHOUT_QUERY);
    }

    /// Clink may ask again once its scripts have loaded. By then a client is
    /// attached, and that answer is the emulator's.
    #[test]
    fn only_the_first_query_is_the_daemons_to_answer() {
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[HANDSHAKE, b"\x1b[c", b"\x1b", b"[c"]);
        assert_eq!(replies, 1);
        assert_eq!(out, [HANDSHAKE_WITHOUT_QUERY, b"\x1b[c\x1b[c"].concat());
    }

    #[test]
    fn a_pane_without_a_fresh_conpty_is_not_touched() {
        let mut query = StartupQuery::new(false);
        let (bytes, asked) = query.strip(HANDSHAKE);
        assert!(!asked);
        assert!(matches!(bytes, Cow::Borrowed(b) if b == HANDSHAKE));
    }

    #[test]
    fn a_query_past_the_startup_window_is_a_programs() {
        let output = vec![b'x'; WINDOW];

        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[&output, QUERY]);
        assert_eq!(replies, 0);
        assert_eq!(out, [&output[..], QUERY].concat());

        // The same bytes in one read: where the query starts is what counts.
        let together = [&output[..], QUERY].concat();
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[&together]);
        assert_eq!(replies, 0);
        assert_eq!(out, together);

        // One byte earlier it starts inside the window.
        let inside = [&output[..WINDOW - 1], QUERY].concat();
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[&inside]);
        assert_eq!(replies, 1);
        assert_eq!(out, &output[..WINDOW - 1]);
    }

    #[test]
    fn an_escape_that_starts_past_the_window_is_not_held_back() {
        let output = vec![b'x'; WINDOW - 1];
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[&output, b"y\x1b", b"[c"]);
        assert_eq!(replies, 0);
        assert_eq!(out, [&output[..], b"y\x1b[c"].concat());
    }

    #[test]
    fn other_device_attribute_requests_are_the_emulators() {
        let mut query = StartupQuery::new(true);
        let (out, replies) = pass(&mut query, &[b"\x1b[>c\x1b[=c\x1b[0c"]);
        assert_eq!(replies, 0);
        assert_eq!(out, b"\x1b[>c\x1b[=c\x1b[0c");
    }

    #[test]
    fn output_after_the_answer_is_borrowed_not_copied() {
        let mut query = StartupQuery::new(true);
        let _ = query.strip(HANDSHAKE);
        let (bytes, asked) = query.strip(b"\x1b[c plain output");
        assert!(!asked);
        assert!(matches!(bytes, Cow::Borrowed(_)));
    }
}

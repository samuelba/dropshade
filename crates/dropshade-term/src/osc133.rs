//! Finds OSC 133 shell integration marks in the pty output.
//!
//! `alacritty_terminal` ignores OSC 133, so the bytes are scanned before they are parsed. The
//! scanner only reports where each mark ends; every byte still goes to the emulator.

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Longest OSC body that is remembered; marks are far shorter.
const MAX_BODY: usize = 256;

/// What an OSC 133 mark says about the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// `A`: the prompt is about to be drawn.
    PromptStart,
    /// `B`: the prompt ended and the user types the command from here.
    CommandStart,
    /// `C`: the command was entered and its output follows.
    OutputStart,
    /// `D`: the command finished, with its exit status when the shell told it.
    CommandEnd(Option<i32>),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc,
}

/// A streaming scanner that keeps its place across reads, so a mark may be split between them.
#[derive(Debug, Default)]
pub struct Osc133Scanner {
    state: State,
    body: Vec<u8>,
}

impl Osc133Scanner {
    /// Calls `found` with the offset just past each complete mark in `bytes` and its kind.
    pub fn scan(&mut self, bytes: &[u8], mut found: impl FnMut(usize, MarkKind)) {
        for (index, &byte) in bytes.iter().enumerate() {
            self.state = match self.state {
                State::Ground if byte == ESC => State::Escape,
                State::Ground => State::Ground,
                State::Escape => match byte {
                    b']' => {
                        self.body.clear();
                        State::Osc
                    }
                    ESC => State::Escape,
                    _ => State::Ground,
                },
                State::Osc => match byte {
                    BEL => {
                        self.finish(index + 1, &mut found);
                        State::Ground
                    }
                    // Like the emulator, any escape ends the string; `ESC \` is the usual one.
                    ESC => {
                        self.finish(index + 1, &mut found);
                        State::Escape
                    }
                    CAN | SUB => State::Ground,
                    _ => {
                        if self.body.len() < MAX_BODY {
                            self.body.push(byte);
                        }
                        State::Osc
                    }
                },
            };
        }
    }

    fn finish(&mut self, end: usize, found: &mut impl FnMut(usize, MarkKind)) {
        if let Some(kind) = parse(&self.body) {
            found(end, kind);
        }
        self.body.clear();
    }
}

/// Reads `133;<kind>[;params]`. Parameters other than the exit status are ignored.
fn parse(body: &[u8]) -> Option<MarkKind> {
    let rest = body.strip_prefix(b"133;")?;
    let mut fields = rest.split(|&byte| byte == b';');
    let kind = match fields.next()? {
        b"A" => MarkKind::PromptStart,
        b"B" => MarkKind::CommandStart,
        b"C" => MarkKind::OutputStart,
        b"D" => MarkKind::CommandEnd(
            fields
                .next()
                .and_then(|status| std::str::from_utf8(status).ok())
                .and_then(|status| status.trim().parse().ok()),
        ),
        _ => return None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(chunks: &[&[u8]]) -> Vec<(usize, usize, MarkKind)> {
        let mut scanner = Osc133Scanner::default();
        let mut found = Vec::new();
        for (chunk, bytes) in chunks.iter().enumerate() {
            scanner.scan(bytes, |end, kind| found.push((chunk, end, kind)));
        }
        found
    }

    #[test]
    fn finds_marks_ended_by_bel_or_string_terminator() {
        let bytes = b"\x1b]133;A\x07$ \x1b]133;B\x1b\\ls";
        assert_eq!(
            marks(&[bytes]),
            [
                (0, 8, MarkKind::PromptStart),
                (0, 18, MarkKind::CommandStart)
            ]
        );
    }

    #[test]
    fn reads_the_exit_status_of_a_finished_command() {
        assert_eq!(
            marks(&[b"\x1b]133;D;127\x07"]),
            [(0, 12, MarkKind::CommandEnd(Some(127)))]
        );
        assert_eq!(
            marks(&[b"\x1b]133;D\x07"]),
            [(0, 8, MarkKind::CommandEnd(None))]
        );
    }

    #[test]
    fn ignores_extra_parameters() {
        assert_eq!(
            marks(&[b"\x1b]133;A;click_events=1;aid=7\x07"]),
            [(0, 29, MarkKind::PromptStart)]
        );
        assert_eq!(
            marks(&[b"\x1b]133;C;cmdline=ls\x07"]),
            [(0, 19, MarkKind::OutputStart)]
        );
    }

    #[test]
    fn finds_a_mark_split_across_reads() {
        assert_eq!(
            marks(&[b"out\x1b]13", b"3;C", b"\x07more"]),
            [(2, 1, MarkKind::OutputStart)]
        );
    }

    #[test]
    fn other_sequences_are_not_marks() {
        assert_eq!(
            marks(&[b"\x1b]0;title\x07\x1b]1330;A\x07\x1b]133;Z\x07\x1b[1mtext"]),
            []
        );
    }

    #[test]
    fn a_cancelled_sequence_is_not_a_mark() {
        assert_eq!(marks(&[b"\x1b]133;A\x18\x07"]), []);
    }
}

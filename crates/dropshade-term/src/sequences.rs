//! Finds the escape sequences `alacritty_terminal` ignores but Dropshade acts on: OSC 133 shell
//! integration marks, kitty graphics commands (`APC G … ST`) and sixel images (`DCS … q … ST`).
//!
//! The bytes are scanned before they are parsed. The scanner only reports where each sequence
//! ends; every byte still goes to the emulator, which skips APC and DCS strings.

use crate::osc133::{self, MarkKind};

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Longest OSC body that is remembered; marks are far shorter.
const MAX_OSC: usize = 256;
/// Longest image sequence that is kept; longer ones are dropped.
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;

/// A sequence found in the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Mark(MarkKind),
    /// The body of a kitty graphics command, after the `G`.
    Kitty(Vec<u8>),
    /// The body of a sixel image: its parameters, then the data after the `q`.
    Sixel {
        params: Vec<u8>,
        data: Vec<u8>,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc,
    /// Just after `ESC _`: a `G` makes it a kitty graphics command.
    ApcStart,
    Kitty,
    /// The parameters of a DCS, until its final byte says what it is.
    DcsParams,
    Sixel,
    /// The rest of a string that is not wanted, or too long, up to its terminator.
    Skip,
    /// The `ESC` that ended an image; the image is reported after the `\` that follows, once
    /// the emulator is back on the ground and Dropshade may feed it bytes of its own.
    Terminator,
}

/// A streaming scanner that keeps its place across reads, so a sequence may be split between
/// them.
#[derive(Debug)]
pub struct SequenceScanner {
    state: State,
    body: Vec<u8>,
    params: Vec<u8>,
    /// An image waiting for the end of its terminator.
    pending: Option<Found>,
    images: bool,
}

impl Default for SequenceScanner {
    fn default() -> Self {
        Self::new(true)
    }
}

impl SequenceScanner {
    /// A scanner that reports image sequences only when `images` is true.
    pub fn new(images: bool) -> Self {
        Self {
            state: State::Ground,
            body: Vec::new(),
            params: Vec::new(),
            pending: None,
            images,
        }
    }

    /// Calls `found` with the offset just past each complete sequence in `bytes` and what it is.
    pub fn scan(&mut self, bytes: &[u8], mut found: impl FnMut(usize, Found)) {
        let mut index = 0;
        while index < bytes.len() {
            if matches!(self.state, State::Kitty | State::Sixel | State::Skip) {
                index = self.take_string(bytes, index);
                if index >= bytes.len() {
                    break;
                }
            }
            let byte = bytes[index];
            index += 1;
            self.state = match self.state {
                State::Ground if byte == ESC => State::Escape,
                State::Ground => State::Ground,
                State::Escape => self.escape(byte),
                State::Osc => self.osc(byte, index, &mut found),
                State::ApcStart => match byte {
                    b'G' if self.images => State::Kitty,
                    ESC => State::Escape,
                    CAN | SUB => State::Ground,
                    _ => State::Skip,
                },
                State::DcsParams => self.dcs_params(byte),
                State::Kitty | State::Sixel | State::Skip => self.end_string(byte),
                State::Terminator => {
                    let terminated = byte == b'\\';
                    if let Some(what) = self.pending.take() {
                        // Without the `\`, the escape starts something else.
                        found(if terminated { index } else { index - 1 }, what);
                    }
                    if terminated {
                        State::Ground
                    } else {
                        self.escape(byte)
                    }
                }
            };
        }
    }

    fn escape(&mut self, byte: u8) -> State {
        self.body.clear();
        match byte {
            b']' => State::Osc,
            b'_' => State::ApcStart,
            b'P' => {
                self.params.clear();
                State::DcsParams
            }
            ESC => State::Escape,
            _ => State::Ground,
        }
    }

    fn osc(&mut self, byte: u8, end: usize, found: &mut impl FnMut(usize, Found)) -> State {
        match byte {
            BEL => {
                self.finish_osc(end, found);
                State::Ground
            }
            // Like the emulator, any escape ends the string; `ESC \` is the usual one.
            ESC => {
                self.finish_osc(end, found);
                State::Escape
            }
            CAN | SUB => State::Ground,
            _ => {
                if self.body.len() < MAX_OSC {
                    self.body.push(byte);
                }
                State::Osc
            }
        }
    }

    fn finish_osc(&mut self, end: usize, found: &mut impl FnMut(usize, Found)) {
        if let Some(kind) = osc133::parse(&self.body) {
            found(end, Found::Mark(kind));
        }
        self.body.clear();
    }

    fn dcs_params(&mut self, byte: u8) -> State {
        match byte {
            b'0'..=b'9' | b';' => {
                if self.params.len() < MAX_OSC {
                    self.params.push(byte);
                }
                State::DcsParams
            }
            b'q' if self.images => State::Sixel,
            ESC => State::Escape,
            CAN | SUB => State::Ground,
            _ => State::Skip,
        }
    }

    /// Takes the bytes of a string up to the next byte that may end it; returns where that is.
    fn take_string(&mut self, bytes: &[u8], start: usize) -> usize {
        let rest = &bytes[start..];
        let len = rest
            .iter()
            .position(|&byte| matches!(byte, ESC | CAN | SUB))
            .unwrap_or(rest.len());
        if self.state != State::Skip {
            if self.body.len() + len > MAX_PAYLOAD {
                self.body = Vec::new();
                self.state = State::Skip;
            } else {
                self.body.extend_from_slice(&rest[..len]);
            }
        }
        start + len
    }

    fn end_string(&mut self, byte: u8) -> State {
        let state = std::mem::replace(&mut self.state, State::Ground);
        let body = std::mem::take(&mut self.body);
        if byte != ESC {
            // Cancelled.
            return State::Ground;
        }
        self.pending = match state {
            State::Kitty => Some(Found::Kitty(body)),
            State::Sixel => Some(Found::Sixel {
                params: std::mem::take(&mut self.params),
                data: body,
            }),
            _ => None,
        };
        if self.pending.is_some() {
            return State::Terminator;
        }
        State::Escape
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_chunks(scanner: &mut SequenceScanner, chunks: &[&[u8]]) -> Vec<(usize, usize, Found)> {
        let mut found = Vec::new();
        for (chunk, bytes) in chunks.iter().enumerate() {
            scanner.scan(bytes, |end, what| found.push((chunk, end, what)));
        }
        found
    }

    fn sequences(chunks: &[&[u8]]) -> Vec<(usize, usize, Found)> {
        scan_chunks(&mut SequenceScanner::default(), chunks)
    }

    fn marks(chunks: &[&[u8]]) -> Vec<(usize, usize, MarkKind)> {
        sequences(chunks)
            .into_iter()
            .filter_map(|(chunk, end, found)| match found {
                Found::Mark(kind) => Some((chunk, end, kind)),
                _ => None,
            })
            .collect()
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
    fn finds_a_mark_split_across_reads() {
        assert_eq!(
            marks(&[b"out\x1b]13", b"3;C", b"\x07more"]),
            [(2, 1, MarkKind::OutputStart)]
        );
    }

    #[test]
    fn other_sequences_are_not_marks() {
        assert_eq!(
            sequences(&[b"\x1b]0;title\x07\x1b]1330;A\x07\x1b]133;Z\x07\x1b[1mtext"]),
            []
        );
    }

    #[test]
    fn a_cancelled_sequence_is_not_a_mark() {
        assert_eq!(marks(&[b"\x1b]133;A\x18\x07"]), []);
    }

    #[test]
    fn finds_a_kitty_graphics_command() {
        let bytes = b"a\x1b_Ga=T,f=100;iVBOR\x1b\\b";
        assert_eq!(
            sequences(&[bytes]),
            [(0, 21, Found::Kitty(b"a=T,f=100;iVBOR".to_vec()))]
        );
    }

    #[test]
    fn finds_a_kitty_command_split_across_reads() {
        assert_eq!(
            sequences(&[b"\x1b", b"_", b"Gi=1;AA", b"AA\x1b", b"\\"]),
            [(4, 1, Found::Kitty(b"i=1;AAAA".to_vec()))]
        );
    }

    #[test]
    fn other_application_strings_are_skipped() {
        assert_eq!(sequences(&[b"\x1b_Xi=1;AAAA\x1b\\\x1b_\x1b\\"]), []);
    }

    #[test]
    fn like_the_emulator_any_escape_ends_a_string() {
        let found = sequences(&[b"\x1b_Ga=T;AA\x1b]133;A\x07"]);
        let kinds: Vec<_> = found.into_iter().map(|(_, _, found)| found).collect();
        assert_eq!(
            kinds,
            [
                Found::Kitty(b"a=T;AA".to_vec()),
                Found::Mark(MarkKind::PromptStart)
            ]
        );
    }

    #[test]
    fn finds_a_sixel_image_with_its_parameters() {
        let bytes = b"\x1bP0;1q\"1;1;2;6#0;2;100;0;0~~\x1b\\";
        assert_eq!(
            sequences(&[bytes]),
            [(
                0,
                bytes.len(),
                Found::Sixel {
                    params: b"0;1".to_vec(),
                    data: b"\"1;1;2;6#0;2;100;0;0~~".to_vec(),
                }
            )]
        );
    }

    #[test]
    fn other_device_control_strings_are_skipped() {
        assert_eq!(sequences(&[b"\x1bP=1s\x1b\\\x1bP$qm\x1b\\"]), []);
    }

    #[test]
    fn marks_and_images_mix() {
        let found = sequences(&[b"\x1b]133;C\x07\x1b_Ga=q;\x1b\\\x1b]133;D;0\x07"]);
        let kinds: Vec<_> = found.into_iter().map(|(_, _, found)| found).collect();
        assert_eq!(
            kinds,
            [
                Found::Mark(MarkKind::OutputStart),
                Found::Kitty(b"a=q;".to_vec()),
                Found::Mark(MarkKind::CommandEnd(Some(0))),
            ]
        );
    }

    #[test]
    fn an_image_ended_by_another_escape_is_reported_before_it() {
        let found = sequences(&[b"\x1b_Ga=T;AA\x1b[1m"]);
        assert_eq!(found, [(0, 10, Found::Kitty(b"a=T;AA".to_vec()))]);
    }

    #[test]
    fn a_cancelled_image_is_dropped() {
        assert_eq!(sequences(&[b"\x1b_Ga=T;AAAA\x18done"]), []);
    }

    #[test]
    fn an_oversized_image_is_dropped_but_later_sequences_are_found() {
        let mut scanner = SequenceScanner::default();
        let chunk = vec![b'A'; MAX_PAYLOAD / 2 + 1];
        let found = scan_chunks(
            &mut scanner,
            &[b"\x1b_Ga=T;", &chunk, &chunk, b"\x1b\\\x1b]133;A\x07"],
        );
        assert_eq!(found, [(3, 10, Found::Mark(MarkKind::PromptStart))]);
    }

    #[test]
    fn images_can_be_turned_off() {
        let mut scanner = SequenceScanner::new(false);
        let found = scan_chunks(&mut scanner, &[b"\x1b_Ga=T;AAAA\x1b\\\x1bPq~\x1b\\"]);
        assert_eq!(found, []);
    }
}

//! Prepares pasted text for the pty.

/// The bytes to send for pasted `text`. With bracketed paste the text is wrapped in markers and
/// stripped of escape characters, so pasted content can never end the paste early or inject
/// control sequences; without it, line breaks become carriage returns as if typed.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let content = text.replace('\x1b', "");
        [b"\x1b[200~", content.as_bytes(), b"\x1b[201~"].concat()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracketed_paste_wraps_text_in_markers() {
        assert_eq!(
            paste_bytes("ls -la\n", true),
            b"\x1b[200~ls -la\n\x1b[201~".to_vec()
        );
    }

    #[test]
    fn bracketed_paste_cannot_be_terminated_by_the_content() {
        let malicious = "echo hi\x1b[201~rm -rf ~\n";
        assert_eq!(
            paste_bytes(malicious, true),
            b"\x1b[200~echo hi[201~rm -rf ~\n\x1b[201~".to_vec()
        );
    }

    #[test]
    fn plain_paste_turns_line_breaks_into_carriage_returns() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }
}

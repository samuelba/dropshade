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

/// `path` quoted for a POSIX-style shell, followed by a space so further drops or typing continue
/// cleanly. Plain paths stay bare; anything else is single-quoted.
pub fn shell_quoted_path(path: &str) -> String {
    let plain = !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    if plain {
        format!("{path} ")
    } else {
        format!("'{}' ", path.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paths_are_left_bare() {
        assert_eq!(
            shell_quoted_path("/home/sam/a-b_c.txt"),
            "/home/sam/a-b_c.txt "
        );
    }

    #[test]
    fn paths_with_spaces_and_metacharacters_are_single_quoted() {
        assert_eq!(
            shell_quoted_path("/tmp/my file $(x).txt"),
            "'/tmp/my file $(x).txt' "
        );
    }

    #[test]
    fn single_quotes_in_paths_are_escaped() {
        assert_eq!(shell_quoted_path("/tmp/it's"), "'/tmp/it'\\''s' ");
    }

    #[test]
    fn empty_path_becomes_an_empty_quoted_string() {
        assert_eq!(shell_quoted_path(""), "'' ");
    }

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

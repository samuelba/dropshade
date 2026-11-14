//! OSC 133 shell integration marks, which `alacritty_terminal` ignores; found by
//! [`crate::sequences::SequenceScanner`].

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

/// Reads an OSC body `133;<kind>[;params]`. Parameters other than the exit status are ignored.
pub(crate) fn parse(body: &[u8]) -> Option<MarkKind> {
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

    #[test]
    fn reads_the_exit_status_of_a_finished_command() {
        assert_eq!(parse(b"133;D;127"), Some(MarkKind::CommandEnd(Some(127))));
        assert_eq!(parse(b"133;D"), Some(MarkKind::CommandEnd(None)));
    }

    #[test]
    fn ignores_extra_parameters() {
        assert_eq!(
            parse(b"133;A;click_events=1;aid=7"),
            Some(MarkKind::PromptStart)
        );
        assert_eq!(parse(b"133;C;cmdline=ls"), Some(MarkKind::OutputStart));
    }

    #[test]
    fn other_kinds_are_not_marks() {
        assert_eq!(parse(b"133;Z"), None);
        assert_eq!(parse(b"1330;A"), None);
        assert_eq!(parse(b"0;title"), None);
    }
}

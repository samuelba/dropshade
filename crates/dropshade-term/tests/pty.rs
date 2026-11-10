use std::time::{Duration, Instant};

use dropshade_term::{GridSize, Palette, SpawnOptions, Terminal};

const TIMEOUT: Duration = Duration::from_secs(5);

fn screen_text(terminal: &Terminal) -> String {
    let frame = terminal.frame(&Palette::default());
    frame
        .runs
        .iter()
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn wait_for(terminal: &Terminal, needle: &str) -> String {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let text = screen_text(terminal);
        if text.contains(needle) || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn spawn(command: &str, size: GridSize) -> Terminal {
    let options = SpawnOptions {
        command: Some(("sh".into(), vec!["-c".into(), command.into()])),
        size,
        ..SpawnOptions::default()
    };
    let (terminal, _events) = Terminal::spawn(options).expect("spawn sh");
    terminal
}

#[test]
fn shell_output_reaches_the_grid() {
    let terminal = spawn(
        "printf 'hello from dropshade'; sleep 2",
        GridSize {
            columns: 40,
            lines: 5,
        },
    );
    let text = wait_for(&terminal, "hello from dropshade");
    assert!(
        text.contains("hello from dropshade"),
        "screen was: {text:?}"
    );
}

#[test]
fn input_is_written_to_the_shell() {
    let terminal = spawn(
        "read line; printf 'got:%s' \"$line\"; sleep 2",
        GridSize {
            columns: 40,
            lines: 5,
        },
    );
    terminal.write(b"ping\r".to_vec());
    let text = wait_for(&terminal, "got:ping");
    assert!(text.contains("got:ping"), "screen was: {text:?}");
}

#[test]
fn resize_is_reported_to_the_child() {
    let mut terminal = spawn(
        "read _; stty size; sleep 2",
        GridSize {
            columns: 40,
            lines: 5,
        },
    );
    terminal.resize(GridSize {
        columns: 33,
        lines: 7,
    });
    terminal.write(b"\r".to_vec());
    let text = wait_for(&terminal, "7 33");
    assert!(text.contains("7 33"), "screen was: {text:?}");
}

mod events {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::sync::Arc;

    use dropshade_term::{GridSize, Notice, Palette, TermEvent};

    use super::spawn;

    #[test]
    fn title_changes_are_tracked() {
        let mut terminal = spawn(
            "sleep 2",
            GridSize {
                columns: 20,
                lines: 3,
            },
        );
        let palette = Palette::default();
        assert_eq!(
            terminal.handle_event(TermEvent::Title("vim".into()), &palette),
            Some(Notice::TitleChanged)
        );
        assert_eq!(terminal.title(), Some("vim"));
        terminal.handle_event(TermEvent::ResetTitle, &palette);
        assert_eq!(terminal.title(), None);
    }

    #[test]
    fn notices_for_redraw_bell_and_exit() {
        let mut terminal = spawn(
            "sleep 2",
            GridSize {
                columns: 20,
                lines: 3,
            },
        );
        let palette = Palette::default();
        assert_eq!(
            terminal.handle_event(TermEvent::Wakeup, &palette),
            Some(Notice::Redraw)
        );
        assert_eq!(
            terminal.handle_event(TermEvent::Bell, &palette),
            Some(Notice::Bell)
        );
        assert_eq!(
            terminal.handle_event(TermEvent::ChildExit(ExitStatus::from_raw(0)), &palette),
            Some(Notice::Exited(Some(0)))
        );
        assert_eq!(
            terminal.handle_event(TermEvent::Exit, &palette),
            Some(Notice::Exited(None))
        );
    }

    #[test]
    fn replies_are_written_back_without_a_notice() {
        let mut terminal = spawn(
            "sleep 2",
            GridSize {
                columns: 20,
                lines: 3,
            },
        );
        let palette = Palette::default();
        let replies = [
            TermEvent::PtyWrite("\x1b[0n".into()),
            TermEvent::TextAreaSizeRequest(Arc::new(|size| {
                format!("{}x{}", size.num_cols, size.num_lines)
            })),
            TermEvent::ColorRequest(1, Arc::new(|rgb| format!("{rgb:?}"))),
        ];
        for reply in replies {
            assert_eq!(terminal.handle_event(reply, &palette), None);
        }
    }

    #[test]
    fn scrolling_and_accessors_work_on_a_live_terminal() {
        let terminal = spawn(
            "seq 1 100; sleep 2",
            GridSize {
                columns: 20,
                lines: 3,
            },
        );
        super::wait_for(&terminal, "100");
        terminal.scroll(5);
        assert!(!super::screen_text(&terminal).contains("100"));
        terminal.scroll_to_bottom();
        assert!(super::screen_text(&terminal).contains("100"));
        assert!(terminal.child_pid() > 0);
        assert!(terminal.cell().width > 0.0);
    }
}

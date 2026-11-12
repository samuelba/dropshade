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
fn reset_clears_the_screen_and_asks_the_shell_to_redraw() {
    let terminal = spawn(
        "exec cat",
        GridSize {
            columns: 40,
            lines: 5,
        },
    );
    terminal.write(b"before\r".to_vec());
    wait_for(&terminal, "before");

    terminal.reset();

    let text = wait_for(&terminal, "^L");
    assert!(!text.contains("before"), "screen was: {text:?}");
    assert!(text.contains("^L"), "no redraw request: {text:?}");
}

#[test]
fn reset_and_clear_asks_the_shell_to_redraw() {
    let mut terminal = spawn(
        "exec cat",
        GridSize {
            columns: 40,
            lines: 5,
        },
    );
    terminal.write(b"before\r".to_vec());
    wait_for(&terminal, "before");

    terminal.reset_and_clear();

    let text = wait_for(&terminal, "^L");
    assert!(!text.contains("before"), "screen was: {text:?}");
    assert!(text.contains("^L"), "no redraw request: {text:?}");
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
    fn notices_for_output_redraw_bell_and_exit() {
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
            Some(Notice::Output)
        );
        assert_eq!(
            terminal.handle_event(TermEvent::MouseCursorDirty, &palette),
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

    #[test]
    fn scroll_to_moves_to_an_absolute_offset_within_the_scrollback() {
        let terminal = spawn(
            "seq 1 100; sleep 2",
            GridSize {
                columns: 20,
                lines: 3,
            },
        );
        super::wait_for(&terminal, "100");
        let live = terminal.scroll_position();
        assert_eq!((live.offset, live.screen), (0, 3));
        assert!(live.history >= 97);
        terminal.scroll_to(40);
        assert_eq!(terminal.scroll_position().offset, 40);
        terminal.scroll_to(10);
        assert_eq!(terminal.scroll_position().offset, 10);
        terminal.scroll_to(usize::MAX);
        assert_eq!(terminal.scroll_position().offset, live.history);
        assert!(super::screen_text(&terminal).starts_with('1'));
        terminal.scroll_to(0);
        assert!(super::screen_text(&terminal).contains("100"));
    }
}

#[test]
fn applications_can_enable_the_kitty_keyboard_protocol() {
    use alacritty_terminal::term::TermMode;

    let terminal = spawn(
        "printf '\\033[>1uready'; sleep 2",
        GridSize {
            columns: 20,
            lines: 3,
        },
    );
    wait_for(&terminal, "ready");
    let screen = screen_text(&terminal);
    let mode = terminal.mode();
    assert!(
        mode.contains(TermMode::DISAMBIGUATE_ESC_CODES),
        "mode {mode:?}, screen {screen:?}"
    );
}

mod selection {
    use dropshade_term::mouse::CellPosition;
    use dropshade_term::size::Side;
    use dropshade_term::{GridSize, SelectionKind};

    use super::{spawn, wait_for};

    const SIZE: GridSize = GridSize {
        columns: 30,
        lines: 3,
    };

    fn at(column: usize) -> CellPosition {
        CellPosition { column, line: 0 }
    }

    #[test]
    fn double_click_selects_a_word() {
        let terminal = spawn("printf 'hello world'; sleep 2", SIZE);
        wait_for(&terminal, "hello world");
        terminal.select(SelectionKind::Word, at(1), Side::Left);
        assert_eq!(terminal.selection_text().as_deref(), Some("hello"));
    }

    #[test]
    fn triple_click_selects_the_line() {
        let terminal = spawn("printf 'hello world'; sleep 2", SIZE);
        wait_for(&terminal, "hello world");
        terminal.select(SelectionKind::Line, at(3), Side::Left);
        let text = terminal.selection_text().unwrap_or_default();
        assert_eq!(text.trim_end(), "hello world");
    }

    #[test]
    fn dragging_extends_a_simple_selection_and_clearing_removes_it() {
        let terminal = spawn("printf 'hello world'; sleep 2", SIZE);
        wait_for(&terminal, "hello world");
        terminal.select(SelectionKind::Simple, at(6), Side::Left);
        terminal.extend_selection(at(10), Side::Right);
        assert_eq!(terminal.selection_text().as_deref(), Some("world"));
        terminal.clear_selection();
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn paste_reaches_the_shell() {
        let terminal = spawn("read line; printf 'got:%s' \"$line\"; sleep 2", SIZE);
        terminal.paste("pasted\n");
        let text = wait_for(&terminal, "got:pasted");
        assert!(text.contains("got:pasted"), "screen was: {text:?}");
    }
}

mod links {
    use dropshade_term::GridSize;
    use dropshade_term::mouse::CellPosition;

    use super::{spawn, wait_for};

    const SIZE: GridSize = GridSize {
        columns: 40,
        lines: 3,
    };

    fn at(column: usize) -> CellPosition {
        CellPosition { column, line: 0 }
    }

    #[test]
    fn plain_urls_in_output_are_found() {
        let terminal = spawn("printf 'visit https://example.com now'; sleep 2", SIZE);
        wait_for(&terminal, "now");
        assert_eq!(
            terminal.link_at(at(8)).as_deref(),
            Some("https://example.com")
        );
        assert_eq!(terminal.link_at(at(2)), None);
    }

    #[test]
    fn osc8_hyperlinks_win_over_the_visible_text() {
        let script =
            "printf '\\033]8;;https://osc.example/\\033\\\\label\\033]8;;\\033\\\\ done'; sleep 2";
        let terminal = spawn(script, SIZE);
        wait_for(&terminal, "done");
        assert_eq!(
            terminal.link_at(at(1)).as_deref(),
            Some("https://osc.example/")
        );
        assert_eq!(terminal.link_at(at(7)), None);
    }

    #[test]
    fn positions_outside_the_grid_have_no_link() {
        let terminal = spawn("sleep 2", SIZE);
        assert_eq!(
            terminal.link_at(CellPosition {
                column: 99,
                line: 0
            }),
            None
        );
    }
}

mod cursor {
    use dropshade_term::GridSize;
    use dropshade_term::mouse::CellPosition;

    use super::{spawn, wait_for};

    const SIZE: GridSize = GridSize {
        columns: 20,
        lines: 3,
    };

    #[test]
    fn cursor_position_follows_output() {
        let terminal = spawn("printf 'hi'; sleep 2", SIZE);
        wait_for(&terminal, "hi");
        assert_eq!(
            terminal.cursor_position(),
            Some(CellPosition { column: 2, line: 0 })
        );
    }

    #[test]
    fn applications_can_request_a_blinking_cursor() {
        let steady = spawn("printf 'a'; sleep 2", SIZE);
        wait_for(&steady, "a");
        assert!(!steady.cursor_blinks());

        let blinking = spawn("printf '\\033[1 qb'; sleep 2", SIZE);
        wait_for(&blinking, "b");
        assert!(blinking.cursor_blinks());
    }
}

mod foreground {
    use std::time::{Duration, Instant};

    use dropshade_term::{GridSize, SpawnOptions, Terminal};

    fn interactive_sh() -> Terminal {
        let options = SpawnOptions {
            command: Some(("sh".into(), vec!["-i".into()])),
            size: GridSize {
                columns: 40,
                lines: 5,
            },
            ..SpawnOptions::default()
        };
        Terminal::spawn(options).expect("spawn sh").0
    }

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn an_idle_shell_has_no_foreground_process() {
        let terminal = interactive_sh();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(terminal.foreground_process(), None);
    }

    #[test]
    fn a_running_command_is_reported_by_name() {
        let terminal = interactive_sh();
        terminal.write(b"sleep 5\r".to_vec());
        let found = wait_until(|| terminal.foreground_process().is_some());
        assert!(found, "no foreground process detected");
        assert_eq!(
            terminal.foreground_process().map(|p| p.name).as_deref(),
            Some("sleep")
        );
    }
}

mod working_directory {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use dropshade_term::{GridSize, SpawnOptions, Terminal};

    #[test]
    fn follows_cd_in_the_shell() {
        let options = SpawnOptions {
            command: Some(("sh".into(), vec!["-i".into()])),
            size: GridSize {
                columns: 40,
                lines: 5,
            },
            ..SpawnOptions::default()
        };
        let terminal = Terminal::spawn(options).expect("spawn sh").0;
        terminal.write(b"cd /tmp\r".to_vec());
        let deadline = Instant::now() + Duration::from_secs(5);
        while terminal.working_directory().as_deref() != Some(Path::new("/tmp"))
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            terminal.working_directory().as_deref(),
            Some(Path::new("/tmp"))
        );
    }
}

mod options {
    use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
    use dropshade_term::{GridSize, Palette, SpawnOptions, Terminal};

    use super::wait_for;

    fn beam() -> CursorStyle {
        CursorStyle {
            shape: CursorShape::Beam,
            blinking: true,
        }
    }

    #[test]
    fn the_default_cursor_style_comes_from_the_options() {
        let options = SpawnOptions {
            command: Some((
                "sh".into(),
                vec!["-c".into(), "printf ready; sleep 2".into()],
            )),
            size: GridSize {
                columns: 20,
                lines: 3,
            },
            cursor_style: beam(),
            ..SpawnOptions::default()
        };
        let terminal = Terminal::spawn(options).expect("spawn").0;
        wait_for(&terminal, "ready");
        let cursor = terminal.frame(&Palette::default()).cursor.expect("cursor");
        assert_eq!(cursor.shape, CursorShape::Beam);
        assert!(terminal.cursor_blinks());
    }

    #[test]
    fn options_can_change_while_running() {
        let terminal = super::spawn(
            "printf ready; sleep 2",
            GridSize {
                columns: 20,
                lines: 3,
            },
        );
        wait_for(&terminal, "ready");
        terminal.update_options(500, beam());
        let cursor = terminal.frame(&Palette::default()).cursor.expect("cursor");
        assert_eq!(cursor.shape, CursorShape::Beam);
    }
}

mod search {
    use dropshade_term::{GridSize, SearchDirection};

    use super::{spawn, wait_for};

    const SIZE: GridSize = GridSize {
        columns: 30,
        lines: 5,
    };

    #[test]
    fn finds_the_newest_match_first_and_steps_to_older_ones() {
        let mut terminal = spawn("printf 'alpha one\\nbeta\\nalpha two\\n'; sleep 2", SIZE);
        wait_for(&terminal, "alpha two");
        assert!(terminal.search("alpha"));
        let newest = terminal.selection_text();
        assert_eq!(newest.as_deref(), Some("alpha"));
        assert!(terminal.search_next(SearchDirection::Older));
        assert!(terminal.search_next(SearchDirection::Newer));
        terminal.clear_search();
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn queries_are_plain_text_with_smart_case() {
        let mut terminal = spawn("printf 'x.y xzy Total\\n'; sleep 2", SIZE);
        wait_for(&terminal, "Total");
        assert!(terminal.search("x.y"));
        assert_eq!(terminal.selection_text().as_deref(), Some("x.y"));
        assert!(terminal.search("total"), "lowercase ignores case");
        assert!(!terminal.search("TOTAL"), "uppercase is case-sensitive");
        assert!(!terminal.search(""));
        assert!(!terminal.search("absent"));
    }
}

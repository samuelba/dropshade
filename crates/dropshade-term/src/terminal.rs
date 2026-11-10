//! A shell running in a pty, with its `alacritty_terminal` emulation state.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use iced::futures::Stream;
use iced::futures::channel::mpsc;

use crate::colors::Palette;
use crate::frame::{self, Frame};
use crate::size::{CellSize, GridSize};

/// Forwards emulator events from the pty thread to the UI.
#[derive(Debug, Clone)]
pub struct EventProxy {
    sender: mpsc::UnboundedSender<Event>,
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        if self.sender.unbounded_send(event).is_err() {
            tracing::trace!("terminal event dropped: the UI side is gone");
        }
    }
}

/// How to start the shell.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Program and arguments. `None` runs the user's login shell.
    pub command: Option<(String, Vec<String>)>,
    pub working_directory: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub scrollback_lines: usize,
    pub size: GridSize,
    pub cell: CellSize,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            command: None,
            working_directory: None,
            env: HashMap::new(),
            scrollback_lines: 10_000,
            size: GridSize {
                columns: 80,
                lines: 24,
            },
            cell: CellSize {
                width: 8.0,
                height: 16.0,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("could not open a pty for the shell: {0}")]
    Pty(#[source] std::io::Error),
    #[error("could not start the terminal I/O loop: {0}")]
    EventLoop(#[source] std::io::Error),
}

/// What the UI should do after an emulator event was handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Redraw,
    TitleChanged,
    Bell,
    Exited(Option<i32>),
}

pub struct Terminal {
    term: Arc<FairMutex<Term<EventProxy>>>,
    sender: EventLoopSender,
    size: GridSize,
    cell: CellSize,
    title: Option<String>,
    child_pid: u32,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("size", &self.size)
            .field("title", &self.title)
            .field("child_pid", &self.child_pid)
            .finish_non_exhaustive()
    }
}

impl Terminal {
    /// Starts the shell and returns the terminal plus the stream of its emulator events.
    ///
    /// # Errors
    ///
    /// Fails when no pty can be opened or the I/O thread cannot be set up.
    pub fn spawn(
        options: SpawnOptions,
    ) -> Result<(Self, impl Stream<Item = Event> + Send + 'static), SpawnError> {
        let (sender, receiver) = mpsc::unbounded();
        let proxy = EventProxy { sender };
        let config = Config {
            scrolling_history: options.scrollback_lines,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &options.size,
            proxy.clone(),
        )));

        let pty_options = tty::Options {
            shell: options
                .command
                .map(|(program, args)| tty::Shell::new(program, args)),
            working_directory: options.working_directory,
            drain_on_exit: true,
            env: options.env,
        };
        let window_size = options.size.window_size(options.cell);
        let pty = tty::new(&pty_options, window_size, 0).map_err(SpawnError::Pty)?;
        let child_pid = pty.child().id();
        let event_loop = EventLoop::new(Arc::clone(&term), proxy, pty, true, false)
            .map_err(SpawnError::EventLoop)?;
        let loop_sender = event_loop.channel();
        event_loop.spawn();

        let terminal = Self {
            term,
            sender: loop_sender,
            size: options.size,
            cell: options.cell,
            title: None,
            child_pid,
        };
        Ok((terminal, receiver))
    }

    pub fn size(&self) -> GridSize {
        self.size
    }

    pub fn cell(&self) -> CellSize {
        self.cell
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn child_pid(&self) -> u32 {
        self.child_pid
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn frame(&self, palette: &Palette) -> Frame {
        frame::build(&self.term.lock(), palette)
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.send(Msg::Input(bytes.into()));
    }

    pub fn resize(&mut self, size: GridSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        self.send(Msg::Resize(size.window_size(self.cell)));
    }

    /// Jumps the viewport back to the live end of the output.
    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// Scrolls the viewport through the scrollback; positive values scroll up.
    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    /// Applies an emulator event and tells the UI how to react.
    pub fn handle_event(&mut self, event: Event, palette: &Palette) -> Option<Notice> {
        match event {
            Event::Wakeup | Event::CursorBlinkingChange | Event::MouseCursorDirty => {
                Some(Notice::Redraw)
            }
            Event::Title(title) => {
                self.title = Some(title);
                Some(Notice::TitleChanged)
            }
            Event::ResetTitle => {
                self.title = None;
                Some(Notice::TitleChanged)
            }
            Event::PtyWrite(text) => {
                self.write(text.into_bytes());
                None
            }
            Event::TextAreaSizeRequest(format) => {
                self.write(format(self.size.window_size(self.cell)).into_bytes());
                None
            }
            Event::ColorRequest(index, format) => {
                let overridden = self.term.lock().colors()[index];
                let color = overridden.unwrap_or_else(|| palette.indexed(index));
                self.write(format(color).into_bytes());
                None
            }
            Event::Bell => Some(Notice::Bell),
            Event::ChildExit(status) => Some(Notice::Exited(status.code())),
            Event::Exit => Some(Notice::Exited(None)),
            Event::ClipboardStore(..) | Event::ClipboardLoad(..) => {
                tracing::debug!(?event, "clipboard requests are not supported yet");
                None
            }
        }
    }

    fn send(&self, msg: Msg) {
        if let Err(error) = self.sender.send(msg) {
            tracing::warn!(%error, "could not reach the terminal I/O loop");
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.send(Msg::Shutdown);
    }
}

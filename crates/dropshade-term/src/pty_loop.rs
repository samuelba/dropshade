//! The pty I/O thread: parses the shell's output into the emulator, writes input, resizes.
//!
//! It stands in for `alacritty_terminal`'s event loop so the output can be split at OSC 133
//! marks, and each mark recorded where the cursor is when the emulator reaches it.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{ChildEvent, EventedPty, Pty};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use nix::errno::Errno;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::wait::{Id, WaitPidFlag, waitid};
use nix::unistd::Pid;

use crate::marks::PromptMarks;
use crate::osc133::{MarkKind, Osc133Scanner};
use crate::terminal::EventProxy;

/// Bytes read, and parsed under one hold of the terminal lock, at a time.
const READ_BUFFER: usize = 0x1_0000;
/// Most bytes parsed before the loop looks at its messages again.
const MAX_READ: usize = 0x10_0000;
/// How soon to look again for the exit status of a child that has exited.
const EXIT_RETRY: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) enum Msg {
    Input(Cow<'static, [u8]>),
    Resize(WindowSize),
    Shutdown,
}

/// Sends messages to the pty thread and wakes it up.
#[derive(Debug, Clone)]
pub(crate) struct PtySender {
    messages: Sender<Msg>,
    wake: Arc<UnixStream>,
}

impl PtySender {
    pub fn send(&self, msg: Msg) -> io::Result<()> {
        self.messages
            .send(msg)
            .map_err(|_| io::Error::new(ErrorKind::BrokenPipe, "the pty thread has stopped"))?;
        wake(&self.wake)
    }
}

fn wake(stream: &UnixStream) -> io::Result<()> {
    match (&*stream).write(&[0]) {
        Err(error) if error.kind() != ErrorKind::WouldBlock => Err(error),
        // A full socket already holds a wakeup.
        _ => Ok(()),
    }
}

/// Starts the pty thread, plus one that notices when the shell exits.
pub(crate) fn spawn(
    pty: Pty,
    term: Arc<FairMutex<Term<EventProxy>>>,
    marks: Arc<Mutex<PromptMarks>>,
    proxy: EventProxy,
) -> io::Result<PtySender> {
    let (wake_sender, wake_receiver) = UnixStream::pair()?;
    wake_sender.set_nonblocking(true)?;
    wake_receiver.set_nonblocking(true)?;
    let wake_sender = Arc::new(wake_sender);
    let (messages, receiver) = mpsc::channel();
    let exited = Arc::new(AtomicBool::new(false));

    let child = i32::try_from(pty.child().id())
        .map(Pid::from_raw)
        .map_err(io::Error::other)?;
    std::thread::Builder::new()
        .name("dropshade child".into())
        .spawn({
            let exited = Arc::clone(&exited);
            let wake_sender = Arc::clone(&wake_sender);
            move || {
                wait_for_exit(child);
                exited.store(true, Ordering::SeqCst);
                let _ = wake(&wake_sender);
            }
        })?;

    let pty_loop = PtyLoop {
        pty,
        term,
        marks,
        proxy,
        messages: receiver,
        wake: wake_receiver,
        exited,
        parser: Processor::default(),
        scanner: Osc133Scanner::default(),
        buffer: vec![0; READ_BUFFER],
        queue: VecDeque::new(),
        written: 0,
        pty_open: true,
    };
    std::thread::Builder::new()
        .name("dropshade pty".into())
        .spawn(move || pty_loop.run())?;
    Ok(PtySender {
        messages,
        wake: wake_sender,
    })
}

/// Blocks until the child exits, leaving it to be reaped by the `Pty`: reaping it here would let
/// its pid be reused while the `Pty` still signals and waits on it.
fn wait_for_exit(child: Pid) {
    while let Err(Errno::EINTR) =
        waitid(Id::Pid(child), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
    {}
}

struct PtyLoop {
    pty: Pty,
    term: Arc<FairMutex<Term<EventProxy>>>,
    marks: Arc<Mutex<PromptMarks>>,
    proxy: EventProxy,
    messages: Receiver<Msg>,
    wake: UnixStream,
    exited: Arc<AtomicBool>,
    parser: Processor<StdSyncHandler>,
    scanner: Osc133Scanner,
    buffer: Vec<u8>,
    /// Input waiting for the pty to accept it; `written` bytes of the first entry are out.
    queue: VecDeque<Cow<'static, [u8]>>,
    written: usize,
    /// False once the shell's side of the pty is closed.
    pty_open: bool,
}

impl PtyLoop {
    fn run(mut self) {
        loop {
            let exited = self.exited.load(Ordering::SeqCst);
            let sync_deadline = self.parser.sync_timeout().sync_timeout();
            let mut timeout =
                sync_deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            if exited {
                timeout = Some(timeout.map_or(EXIT_RETRY, |timeout| timeout.min(EXIT_RETRY)));
            }
            let (woken, pty_events) = match self.wait(timeout) {
                Ok(ready) => ready,
                Err(error) => {
                    tracing::error!(%error, "could not wait for the pty");
                    break;
                }
            };
            if sync_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                self.stop_sync();
            }
            if woken {
                self.drain_wake();
            }
            if !self.handle_messages() {
                break;
            }
            if let Some(events) = pty_events {
                if events.intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR)
                    && let Err(error) = self.read()
                {
                    tracing::warn!(%error, "could not read from the pty");
                    self.pty_open = false;
                }
                if events.contains(PollFlags::POLLOUT) {
                    self.flush();
                }
            }
            if exited && self.finish_exit() {
                break;
            }
        }
    }

    /// Waits until the pty or the wake socket is ready, or the timeout passed.
    fn wait(&self, timeout: Option<Duration>) -> nix::Result<(bool, Option<PollFlags>)> {
        let timeout = timeout.map_or(PollTimeout::NONE, |timeout| {
            PollTimeout::try_from(timeout).unwrap_or(PollTimeout::MAX)
        });
        let mut interest = PollFlags::POLLIN;
        if !self.queue.is_empty() {
            interest |= PollFlags::POLLOUT;
        }
        let mut fds = vec![PollFd::new(self.wake.as_fd(), PollFlags::POLLIN)];
        if self.pty_open {
            fds.push(PollFd::new(self.pty.file().as_fd(), interest));
        }
        match poll(&mut fds, timeout) {
            Err(Errno::EINTR) => return Ok((false, None)),
            result => result?,
        };
        let woken = fds[0]
            .revents()
            .is_some_and(|events| events.contains(PollFlags::POLLIN));
        Ok((woken, fds.get(1).and_then(PollFd::revents)))
    }

    fn drain_wake(&mut self) {
        let mut sink = [0; 64];
        while matches!((&self.wake).read(&mut sink), Ok(read) if read > 0) {}
    }

    /// Applies the queued messages; false once the terminal is gone.
    fn handle_messages(&mut self) -> bool {
        loop {
            match self.messages.try_recv() {
                Ok(Msg::Input(bytes)) => {
                    if self.pty_open {
                        self.queue.push_back(bytes);
                    }
                }
                Ok(Msg::Resize(size)) => self.pty.on_resize(size),
                Ok(Msg::Shutdown) | Err(TryRecvError::Disconnected) => return false,
                Err(TryRecvError::Empty) => break,
            }
        }
        self.flush();
        true
    }

    /// Writes queued input until the pty would block.
    fn flush(&mut self) {
        while let Some(front) = self.queue.front() {
            let remaining = &front[self.written..];
            match self.pty.file().write(remaining) {
                Ok(written) => {
                    self.written += written;
                    if written == remaining.len() {
                        self.queue.pop_front();
                        self.written = 0;
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!(%error, "could not write to the pty");
                    self.queue.clear();
                    self.written = 0;
                    break;
                }
            }
        }
    }

    /// Reads and parses what the shell printed; returns how many bytes that was.
    fn read(&mut self) -> io::Result<usize> {
        let mut processed = 0;
        while processed < MAX_READ {
            let read = match self.pty.file().read(&mut self.buffer) {
                Ok(0) => {
                    self.pty_open = false;
                    break;
                }
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                // Linux reports EIO once the shell's side of the pty is closed.
                Err(error) if error.raw_os_error() == Some(Errno::EIO as i32) => {
                    self.pty_open = false;
                    break;
                }
                Err(error) => return Err(error),
            };
            self.parse(read);
            processed += read;
        }
        if processed > 0 && self.parser.sync_bytes_count() < processed {
            self.proxy.send_event(Event::Wakeup);
        }
        Ok(processed)
    }

    /// Feeds the first `len` bytes of the buffer to the emulator, recording each mark in it.
    fn parse(&mut self, len: usize) {
        let bytes = &self.buffer[..len];
        let mut found: Vec<(usize, MarkKind)> = Vec::new();
        self.scanner
            .scan(bytes, |end, kind| found.push((end, kind)));

        let mut term = self.term.lock();
        let mut marks = self.marks.lock().unwrap_or_else(PoisonError::into_inner);
        let mut start = 0;
        for (end, kind) in found {
            advance(&mut self.parser, &mut term, &mut marks, &bytes[start..end]);
            if self.parser.sync_timeout().sync_timeout().is_some() {
                // The mark's place is only known once the buffered update is applied.
                let probe = PromptMarks::begin(&mut term);
                self.parser.stop_sync(&mut *term);
                marks.finish(&mut term, probe);
            }
            marks.record(&term, kind);
            start = end;
        }
        advance(&mut self.parser, &mut term, &mut marks, &bytes[start..]);
    }

    /// Applies a synchronized update whose time ran out.
    fn stop_sync(&mut self) {
        let mut term = self.term.lock();
        let mut marks = self.marks.lock().unwrap_or_else(PoisonError::into_inner);
        let probe = PromptMarks::begin(&mut term);
        self.parser.stop_sync(&mut *term);
        marks.finish(&mut term, probe);
        drop((term, marks));
        self.proxy.send_event(Event::Wakeup);
    }

    /// Reports the exit once the child's status is in; true when the loop should end.
    fn finish_exit(&mut self) -> bool {
        let Some(ChildEvent::Exited(status)) = self.pty.next_child_event() else {
            return false;
        };
        if let Some(status) = status {
            self.proxy.send_event(Event::ChildExit(status));
        }
        while self.pty_open && matches!(self.read(), Ok(read) if read > 0) {}
        self.term.lock().exit();
        self.proxy.send_event(Event::Wakeup);
        true
    }
}

fn advance(
    parser: &mut Processor<StdSyncHandler>,
    term: &mut Term<EventProxy>,
    marks: &mut PromptMarks,
    bytes: &[u8],
) {
    if bytes.is_empty() {
        return;
    }
    let probe = PromptMarks::begin(term);
    parser.advance(term, bytes);
    marks.finish(term, probe);
}

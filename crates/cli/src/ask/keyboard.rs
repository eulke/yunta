//! How a console reads its keyboard and holds its cursor.
//!
//! A key is read in ticks: a read waits [`TICK_MS`] for a key and then
//! looks up, so a prompt that was stopped hears it within a tick, and a
//! terminal that changed size is drawn again within one. Nothing else
//! waits on the keyboard, so a stop never has to wait for a key.

use std::os::fd::AsFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use console::Key;
use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use nix::sys::termios::{tcflush, FlushArg};

use super::Console;

/// How long a read of the keyboard waits before it looks up: at a stop, or at a
/// terminal that changed size. A tenth of a second is a stop nobody waits on, and a
/// wake nobody pays for.
const TICK_MS: u8 = 100;

impl Console {
    /// The flag that stops any prompt on this console.
    pub(crate) fn stopper(&self) -> Arc<AtomicBool> {
        self.stopped.clone()
    }

    /// Discards what was typed and not read yet: asked before a question is drawn,
    /// so a key pressed before it can be read cannot answer it.
    pub(crate) fn discard_typed(&self) {
        // best-effort: a terminal that will not flush loses nothing but this guard.
        tcflush(std::io::stdin(), FlushArg::TCIFLUSH).ok();
    }

    /// The next key, or `None` when a tick went by with none. A stopped prompt reads
    /// as interrupted, which is what it is.
    ///
    /// A typed Ctrl-C arrives here as [`Key::CtrlC`], since a prompt holds the terminal
    /// with its signal keys off, and [`Console::interrupt`] is what puts it on the one
    /// path that stops a run. A signal from outside stops the prompt through the run's
    /// cancellation, and reads here as a stop.
    pub(crate) fn next_key(&self) -> std::io::Result<Option<Key>> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let stdin = std::io::stdin();
        let mut waiting = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
        match poll(&mut waiting, PollTimeout::from(TICK_MS)) {
            Ok(0) | Err(Errno::EINTR) => Ok(None),
            Ok(_) => self.term.read_key_raw().map(Some),
            Err(failed) => Err(failed.into()),
        }
    }

    /// The columns and the rows of this terminal.
    pub(crate) fn size(&self) -> (usize, usize) {
        let (rows, columns) = self.term.size();
        (usize::from(columns), usize::from(rows))
    }
}

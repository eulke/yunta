//! The terminal a region draws on: a person's, redrawn at most so many times a
//! second, each drawing one update of it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use indicatif::{ProgressDrawTarget, TermLike};

/// How many times a second the region may redraw on a terminal a person
/// is watching. The painter has a beat of its own, once a second; this
/// is the ceiling for the redraws events ask for in between, so a node
/// emitting a burst of messages costs the terminal a handful of
/// repaints rather than one per message.
const REDRAW_CEILING_HZ: u8 = 20;

/// The terminal the region draws on and measures its rows against.
///
/// One object answers both questions — where a row goes, and how many
/// cells it may take — so a row is never cut to a width the screen it
/// lands on does not have. The handle is shared: what indicatif draws
/// through and what the region measures with are the same terminal, so
/// a window resized mid-run moves both.
#[derive(Clone, Debug)]
pub(crate) struct Screen {
    term: Arc<dyn TermLike>,
    /// How many times a second this screen accepts a redraw, `None`
    /// taking every one of them.
    hz: Option<u8>,
    /// Whether an update of the terminal is open: shared by every clone of
    /// this screen, since indicatif draws through a clone of it.
    drawing: Arc<AtomicBool>,
}

impl Screen {
    /// The terminal a person is watching, redrawn at most
    /// [`REDRAW_CEILING_HZ`] times a second.
    pub(crate) fn watched(term: impl TermLike + 'static) -> Self {
        Self {
            term: Arc::new(term),
            hz: Some(REDRAW_CEILING_HZ),
            drawing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A screen every draw reaches, for a test that asserts on the rows
    /// one draw produced: under a ceiling the second of two draws made
    /// back to back is dropped, and the assertion after it would be
    /// reading the first.
    #[cfg(test)]
    pub(crate) fn immediate(term: impl TermLike + 'static) -> Self {
        Self {
            term: Arc::new(term),
            hz: None,
            drawing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Where indicatif draws this screen.
    pub(super) fn target(&self) -> ProgressDrawTarget {
        match self.hz {
            Some(hz) => ProgressDrawTarget::term_like_with_hz(Box::new(self.clone()), hz),
            None => ProgressDrawTarget::term_like(Box::new(self.clone())),
        }
    }

    /// Opens one update of the terminal, unless one is open.
    fn opening(&self) -> std::io::Result<()> {
        match self.drawing.swap(true, Ordering::Relaxed) {
            true => Ok(()),
            false => self.term.write_str(surco::paint::SYNC_BEGIN),
        }
    }

    /// Says `text` to the terminal itself, in no update: the window's title and
    /// its bell are no part of a drawing.
    pub(super) fn signal(&self, text: &str) -> std::io::Result<()> {
        self.term.write_str(text)?;
        self.term.flush()
    }
}

impl TermLike for Screen {
    fn width(&self) -> u16 {
        self.term.width()
    }

    fn height(&self) -> u16 {
        self.term.height()
    }

    fn move_cursor_up(&self, n: usize) -> std::io::Result<()> {
        self.opening()?;
        self.term.move_cursor_up(n)
    }

    fn move_cursor_down(&self, n: usize) -> std::io::Result<()> {
        self.opening()?;
        self.term.move_cursor_down(n)
    }

    fn move_cursor_right(&self, n: usize) -> std::io::Result<()> {
        self.opening()?;
        self.term.move_cursor_right(n)
    }

    fn move_cursor_left(&self, n: usize) -> std::io::Result<()> {
        self.opening()?;
        self.term.move_cursor_left(n)
    }

    fn write_line(&self, line: &str) -> std::io::Result<()> {
        self.opening()?;
        self.term.write_line(line)
    }

    fn write_str(&self, text: &str) -> std::io::Result<()> {
        self.opening()?;
        self.term.write_str(text)
    }

    fn clear_line(&self) -> std::io::Result<()> {
        self.opening()?;
        self.term.clear_line()
    }

    fn flush(&self) -> std::io::Result<()> {
        if self.drawing.swap(false, Ordering::Relaxed) {
            self.term.write_str(surco::paint::SYNC_END)?;
        }
        self.term.flush()
    }
}

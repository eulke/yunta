//! What a person sees while a run happens.
//!
//! Three surfaces over one model. Every one of them presents the same
//! [`RunFrame`](yunta_engine::RunFrame) — a derived snapshot of the run,
//! pure in its own log — and they differ only in how it is delivered: a
//! single line and an exit code, one append-only line per event, or a
//! pinned region of plain text that redraws in place. A reader moving
//! between them meets the same facts in the same words, because there is
//! one derivation and one vocabulary behind all three.
//!
//! **The events come from the engine, not from the log.** The engine
//! hands every append to the observer in the moment it writes it
//! ([`RunObserver`]), so the process drawing a run never asks storage for
//! what it wrote an instant earlier. The log stays the truth: what
//! the surface holds is the cache of one invocation, and every hole in it
//! is closed by reading the log.

mod closing;
mod feed;
mod fold;
mod lines;
mod painter;
mod region;
mod scrollback;
mod turns;
mod view;

use std::io::{IsTerminal, Write};
use std::sync::Arc;

use dialoguer::console::Term;
use indicatif::{ProgressDrawTarget, TermLike};
use tokio::task::JoinHandle;

use yunta_core::{Clock, Manifest, RunId};
use yunta_engine::{PriorEstimation, RunObserver};
use yunta_storage::{AsyncStorage, StorageError};

use crate::render::Glyphs;

use feed::{Beat, Feed};
use lines::Lines;
use painter::{Draw, Painter};
use region::Region;

pub(crate) use closing::{Closing, ClosingEnv, Outline};
pub(crate) use feed::Diagnostics;
pub(crate) use turns::Curtain;

/// Why the live region stood down, in the words the reader is given.
/// Each one names a property of the environment this process was handed,
/// never a guess about the terminal at the other end.
const NOT_A_TERMINAL: &str = "stderr is not a terminal";
const DUMB_TERMINAL: &str = "TERM=dumb";
const COLOR_REFUSED: &str = "NO_COLOR is set";
/// Not a property of the environment but of this binary: the region's own
/// row template is a constant it carries, and a build where that constant
/// does not parse still owes the reader every event.
const ROW_TEMPLATE: &str = "this build's region template does not parse";

/// How many events the queue between the engine and the painter holds
/// before the observer starts dropping them. Named once, because the
/// depth and the drop policy are one decision: deep enough that a burst
/// from several parallel nodes lands whole, shallow enough that a painter
/// that fell behind goes back to the log instead of replaying a stale
/// minute of it.
const QUEUE_DEPTH: usize = 1024;

/// How many times a second the region may redraw on a terminal a person
/// is watching. The painter has a beat of its own, once a second; this
/// is the ceiling for the redraws events ask for in between, so a node
/// emitting a burst of messages costs the terminal a handful of
/// repaints rather than one per message.
const REDRAW_CEILING_HZ: u8 = 20;

/// How a run's progress reaches the person who started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// One line carrying the run id, and nothing else — the verdict
    /// travels in the exit code. Quiet is about progress, never about
    /// problems: a diagnostic and the budget warning are printed either
    /// way, because one describes what happened and the other asks for a
    /// decision before anything is spent.
    Quiet,
    /// One line per event, appended, never redrawn, each carrying the
    /// run's elapsed time at that event.
    Lines { reason: &'static str },
    /// A pinned region of plain text at the bottom of the terminal, with
    /// finished work graduating above it into the terminal's own history.
    Live,
}

/// The three values the delivery policy reads, lifted out of the process
/// so the policy is a function of its arguments and nothing else.
pub(crate) struct TerminalEnv {
    /// Whether the stream the region would draw on is a terminal.
    pub(crate) stderr_is_terminal: bool,
    /// `TERM`.
    pub(crate) term: Option<String>,
    /// `NO_COLOR`, which the convention reads as set when it is present
    /// and not empty.
    pub(crate) no_color: Option<String>,
}

impl TerminalEnv {
    /// What this process was started with.
    pub(crate) fn from_process() -> Self {
        Self {
            stderr_is_terminal: std::io::stderr().is_terminal(),
            term: std::env::var("TERM").ok(),
            no_color: std::env::var("NO_COLOR").ok(),
        }
    }
}

impl Delivery {
    /// The delivery `env` allows, with `quiet` settling it outright.
    ///
    /// Three signals decide the rest, in the order a reader would: a
    /// stream that is not a terminal has nowhere to pin a region; a
    /// terminal that declares itself dumb has said it draws nothing
    /// beyond text; and a reader who asked for no color asked for output
    /// that reads the same however it is captured.
    pub(crate) fn choose(quiet: bool, env: &TerminalEnv) -> Self {
        if quiet {
            return Self::Quiet;
        }
        if !env.stderr_is_terminal {
            return Self::Lines {
                reason: NOT_A_TERMINAL,
            };
        }
        if env.term.as_deref() == Some("dumb") {
            return Self::Lines {
                reason: DUMB_TERMINAL,
            };
        }
        if env.no_color.as_deref().is_some_and(|set| !set.is_empty()) {
            return Self::Lines {
                reason: COLOR_REFUSED,
            };
        }
        Self::Live
    }
}

/// Everything the surface needs of the run it draws.
pub(crate) struct SurfaceEnv<'a> {
    pub(crate) run_id: &'a RunId,
    /// The manifest this run froze — the workflow whose nodes it draws,
    /// and what rebuilds the menu a parked run stopped on.
    pub(crate) manifest: &'a Manifest,
    /// What this workflow's past runs cost, for the closing block's own
    /// comparison. `None` below the history floor.
    pub(crate) prior: Option<&'a PriorEstimation>,
    /// Where a hole in the delivered stream is closed.
    pub(crate) storage: &'a AsyncStorage,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) delivery: Delivery,
    /// The characters this process may draw with, read once by the
    /// invocation so the region and the block that closes it cannot
    /// disagree — and so an unusable override is reported once.
    pub(crate) glyphs: Glyphs,
}

/// The live surface of one invocation: the observer the engine feeds and
/// the task that draws from it.
pub(crate) struct Surface {
    feed: Option<Arc<Feed>>,
    /// Kept so the invocation owns the task it spawned and waits for it
    /// to take the region down before anything else prints.
    painter: Option<JoinHandle<()>>,
    /// What whoever else draws on this stream takes its turn with.
    curtain: Curtain,
}

impl Surface {
    /// Opens the surface for `env.run_id`, seeded with the log as it
    /// stands: the run's creation is written before any observer exists,
    /// so the first thing a surface knows it reads once, here.
    pub(crate) async fn open(env: SurfaceEnv<'_>) -> Result<Self, StorageError> {
        let draw = match env.delivery {
            Delivery::Quiet => {
                return Ok(Self {
                    feed: None,
                    painter: None,
                    curtain: Curtain::none(),
                })
            }
            Delivery::Lines { reason } => Draw::Lines(Lines::open(stderr(), reason)),
            Delivery::Live => {
                let screen = Screen::watched(Term::buffered_stderr());
                match Region::open(screen, env.glyphs, stderr()) {
                    Ok(region) => Draw::Live(Box::new(region)),
                    Err(_) => Draw::Lines(Lines::open(stderr(), ROW_TEMPLATE)),
                }
            }
        };
        let seed = env.storage.events_for_run(env.run_id.clone()).await?;
        let (feed, beats) = Feed::open();
        let (curtain, standby) = Curtain::open();
        let painter = Painter::seeded(&env, draw, seed);
        Ok(Self {
            feed: Some(feed),
            painter: Some(tokio::spawn(painter::paint(painter, beats, standby))),
            curtain,
        })
    }

    /// Where a diagnostic raised while this run is drawn goes out.
    ///
    /// Above the region rather than around it, and held while a prompt
    /// has the terminal — the same turn-taking every other line on this
    /// stream observes. A run drawing nothing hands back a door onto
    /// stderr, so the caller has one path either way.
    pub(crate) fn diagnostics(&self) -> Diagnostics {
        self.feed
            .as_ref()
            .map_or_else(Diagnostics::none, Diagnostics::through)
    }

    /// What a prompt on this run's terminal takes its turn with.
    ///
    /// Both surfaces answer it — the region comes off the screen, and
    /// the append-only lines are held back — because either one written
    /// under an open prompt lands on the rows a person is reading.
    pub(crate) fn curtain(&self) -> Curtain {
        self.curtain.clone()
    }

    /// The observer to hand the engine, or `None` when nothing is drawing.
    pub(crate) fn observer(&self) -> Option<Arc<dyn RunObserver>> {
        self.feed
            .as_ref()
            .map(|feed| Arc::clone(feed) as Arc<dyn RunObserver>)
    }

    /// Takes the surface down: everything already queued is drawn, then
    /// the region is cleared, so the closing block lands on a terminal
    /// with nothing pinned to it.
    pub(crate) async fn close(self) {
        if let Some(feed) = &self.feed {
            // Whether the painter took it is nothing to act on here: one
            // that has already stopped takes nothing, and that is the
            // state this instruction is asking for.
            feed.tell(Beat::Close).await;
        }
        if let Some(painter) = self.painter {
            if let Err(e) = painter.await {
                // The painter only ever draws, so a run is unaffected by
                // one that stopped early — but a reader who saw progress
                // freeze deserves to know it was not the run.
                crate::error::warn(format!("the task drawing this run stopped early: {e}"));
            }
        }
    }
}

/// Where a surface writes: stderr, which leaves stdout to carry the run's
/// own result to whatever is reading it.
fn stderr() -> Box<dyn Write + Send> {
    Box::new(std::io::stderr())
}

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
}

impl Screen {
    /// The terminal a person is watching, redrawn at most
    /// [`REDRAW_CEILING_HZ`] times a second.
    pub(crate) fn watched(term: impl TermLike + 'static) -> Self {
        Self {
            term: Arc::new(term),
            hz: Some(REDRAW_CEILING_HZ),
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
        }
    }

    /// Where indicatif draws this screen.
    fn target(&self) -> ProgressDrawTarget {
        match self.hz {
            Some(hz) => ProgressDrawTarget::term_like_with_hz(Box::new(self.clone()), hz),
            None => ProgressDrawTarget::term_like(Box::new(self.clone())),
        }
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
        self.term.move_cursor_up(n)
    }

    fn move_cursor_down(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_down(n)
    }

    fn move_cursor_right(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_right(n)
    }

    fn move_cursor_left(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_left(n)
    }

    fn write_line(&self, line: &str) -> std::io::Result<()> {
        self.term.write_line(line)
    }

    fn write_str(&self, text: &str) -> std::io::Result<()> {
        self.term.write_str(text)
    }

    fn clear_line(&self) -> std::io::Result<()> {
        self.term.clear_line()
    }

    fn flush(&self) -> std::io::Result<()> {
        self.term.flush()
    }
}

/// Writes one line and flushes it, so a reader watching a log sees it
/// now rather than when a buffer fills.
fn write_line(out: &mut Box<dyn Write + Send>, line: &str) {
    // A failed write ends here: the stream a report would have gone out
    // on is the one that just refused this line.
    drop(writeln!(out, "{line}").and_then(|()| out.flush()));
}

/// A terminal a test reads back twice over: what is on its screen, and
/// every row the surface ever wrote to it.
///
/// The second question is the one turn-taking asks. A region clears its
/// own rows as it comes down, so the screen forgets what was drawn on
/// it — and "nothing was drawn while the prompt had the terminal" is
/// exactly what a screen read afterwards cannot answer.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct Watched {
    term: indicatif::InMemoryTerm,
    written: yunta_testkit::Captured,
}

#[cfg(test)]
impl Watched {
    /// A terminal `rows` by `columns`, with nothing on it.
    pub(crate) fn sized(rows: u16, columns: u16) -> Self {
        Self {
            term: indicatif::InMemoryTerm::new(rows, columns),
            written: yunta_testkit::Captured::default(),
        }
    }

    /// What is on the screen now.
    pub(crate) fn shown(&self) -> String {
        self.term.contents()
    }

    /// Every row the surface has written here, in order.
    pub(crate) fn written(&self) -> String {
        self.written.text()
    }

    /// A row written by something other than the surface — what a
    /// prompt drawing on the same terminal puts there. It is not
    /// recorded, so what [`Watched::written`] answers stays the
    /// surface's own writing.
    pub(crate) fn interject(&self, line: &str) -> std::io::Result<()> {
        self.term.write_line(line)
    }

    fn note(&self, text: &str) {
        use std::io::Write as _;
        let mut written = self.written.clone();
        drop(written.write_all(text.as_bytes()));
    }
}

#[cfg(test)]
impl TermLike for Watched {
    fn width(&self) -> u16 {
        self.term.width()
    }

    fn height(&self) -> u16 {
        self.term.height()
    }

    fn move_cursor_up(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_up(n)
    }

    fn move_cursor_down(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_down(n)
    }

    fn move_cursor_right(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_right(n)
    }

    fn move_cursor_left(&self, n: usize) -> std::io::Result<()> {
        self.term.move_cursor_left(n)
    }

    fn write_line(&self, line: &str) -> std::io::Result<()> {
        self.note(line);
        self.term.write_line(line)
    }

    fn write_str(&self, text: &str) -> std::io::Result<()> {
        self.note(text);
        self.term.write_str(text)
    }

    fn clear_line(&self) -> std::io::Result<()> {
        self.term.clear_line()
    }

    fn flush(&self) -> std::io::Result<()> {
        self.term.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(stderr_is_terminal: bool, term: Option<&str>, no_color: Option<&str>) -> TerminalEnv {
        TerminalEnv {
            stderr_is_terminal,
            term: term.map(str::to_string),
            no_color: no_color.map(str::to_string),
        }
    }

    #[test]
    fn quiet_settles_the_delivery_whatever_the_terminal_offers() {
        let terminal = env(true, Some("xterm-256color"), None);
        assert_eq!(Delivery::choose(true, &terminal), Delivery::Quiet);
    }

    #[test]
    fn a_terminal_on_stderr_gets_the_region() {
        let terminal = env(true, Some("xterm-256color"), None);
        assert_eq!(Delivery::choose(false, &terminal), Delivery::Live);
    }

    #[test]
    fn every_downgrade_names_what_was_missing() {
        for (env, reason) in [
            (env(false, Some("xterm"), None), NOT_A_TERMINAL),
            (env(true, Some("dumb"), None), DUMB_TERMINAL),
            (env(true, Some("xterm"), Some("1")), COLOR_REFUSED),
        ] {
            assert_eq!(Delivery::choose(false, &env), Delivery::Lines { reason });
        }
    }

    #[test]
    fn an_empty_no_color_is_not_a_reader_asking_for_anything() {
        let terminal = env(true, Some("xterm"), Some(""));
        assert_eq!(Delivery::choose(false, &terminal), Delivery::Live);
    }
}

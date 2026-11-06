//! What a person sees while a run happens.
//!
//! Three surfaces over two derivations and one vocabulary. The engine
//! derives from the log where each thing stands
//! ([`RunFrame`](yunta_engine::RunFrame)) and what happened, in order
//! ([`yunta_engine::chronicle`]); this module's own `chronicle` chooses
//! a moment's words once; each surface only lays them out. The region
//! draws what is open, the history above it keeps what closed or asked,
//! and the append-only surface writes every moment. A reader moving
//! between them meets the same facts in the same words.
//!
//! **The events come from whoever has them first.** In the process that
//! drives a run, the engine hands every append to the observer in the moment
//! it writes it ([`RunObserver`]), so that process never asks storage for what
//! it wrote an instant earlier. A process that follows a run another one drives
//! reads the log past what it holds and hands each event over itself
//! ([`Surface::deliver`]), with the engine read from the run's registry
//! ([`EngineReading`]). The log stays the truth: what the surface holds is the
//! cache of one invocation, and every hole in it is closed by reading the log.

mod chronicle;
mod closing;
mod draw;
mod feed;
mod fold;
mod lines;
mod painter;
mod region;
mod screen;
mod scrollback;
pub(crate) mod terminal;
mod turns;
mod view;

use std::io::Write;
use std::sync::Arc;

use console::Term;
#[cfg(test)]
use indicatif::TermLike;
use tokio::task::JoinHandle;

use yunta_core::events::StoredEvent;
use yunta_core::{Clock, Manifest, RunId};
use yunta_engine::{Engine, PriorEstimation, RunObserver};
use yunta_storage::AsyncStorage;

use surco::glyphs::Glyphs;

use feed::{Beat, Feed};
use lines::Lines;
use painter::{Draw, Painter};
use region::Region;

pub(crate) use closing::{Closing, ClosingEnv, Outline};
pub(crate) use feed::Diagnostics;
pub(crate) use screen::Screen;
pub(crate) use terminal::TerminalEnv;
pub(crate) use turns::Curtain;
pub(crate) use view::{child_row, children_by_node};

/// Why the live region stood down, in the words the reader is given.
/// Each one names a property of the environment this process was handed,
/// never a guess about the terminal at the other end.
const NOT_A_TERMINAL: &str = "stderr is not a terminal";
const DUMB_TERMINAL: &str = "TERM=dumb";
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

/// How a run's progress reaches the person who started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// One line carrying the run id on stdout, and one line of verdict on stderr
    /// when the run ends. Quiet is about progress, never about problems: a
    /// diagnostic and the budget warning are printed either way, because one
    /// describes what happened and the other asks for a decision before
    /// anything is spent.
    Quiet,
    /// One line per event, appended, never redrawn, each carrying the
    /// run's elapsed time at that event.
    Lines { reason: &'static str },
    /// A pinned region of text at the bottom of the terminal, with
    /// finished work graduating above it into the terminal's own history.
    Live,
}

impl Delivery {
    /// The delivery `env` allows, with `quiet` settling it outright.
    ///
    /// Two signals decide the rest, in the order a reader would: a
    /// stream that is not a terminal has nowhere to pin a region, and a
    /// terminal that declares itself dumb has said it draws nothing
    /// beyond text. A reader who asked for no color asked for exactly
    /// that: the region's color only repeats its words, so `NO_COLOR`
    /// takes away the color and leaves the region.
    pub(crate) fn choose(quiet: bool, env: &TerminalEnv) -> Self {
        if quiet {
            return Self::Quiet;
        }
        if !env.stderr_is_terminal {
            return Self::Lines {
                reason: NOT_A_TERMINAL,
            };
        }
        if env.is_dumb() {
            return Self::Lines {
                reason: DUMB_TERMINAL,
            };
        }
        Self::Live
    }
}

/// Where the frame a surface draws reads the engine driving its run from.
///
/// The process that drives a run is alive, and asks its own terminal, so it
/// reads itself. A process that follows a run another one drives reads the
/// run's registry, which is what says that engine is asking at its terminal or
/// is gone.
#[derive(Debug, Clone)]
pub(crate) enum EngineReading {
    /// This process drives the run.
    This,
    /// Another process drives it: the run's registry, under this run directory.
    Registry(std::path::PathBuf),
}

impl EngineReading {
    /// The same reading for `successor`, the run a promotion handed the work
    /// to. This process drives a successor as it drove its predecessor; a
    /// successor's registry sits beside its predecessor's, since the engine
    /// that promotes creates it under the runs root it works in.
    pub(crate) fn of_successor(&self, successor: &RunId) -> Self {
        match self {
            EngineReading::This => EngineReading::This,
            EngineReading::Registry(run_dir) => {
                EngineReading::Registry(run_dir.with_file_name(successor.as_str()))
            }
        }
    }

    /// The engine driving the run, as this reading says at `at`.
    pub(crate) fn read(&self, at: chrono::DateTime<chrono::Utc>) -> Engine {
        match self {
            EngineReading::This => Engine::this(at),
            EngineReading::Registry(run_dir) => crate::render::engine(run_dir, at),
        }
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
    /// Where the frame reads the engine driving the run from.
    pub(crate) engine: EngineReading,
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
    /// Opens the surface for `env.run_id`, seeded with `seed`, the run's log
    /// as the caller read it. The caller reads it because what comes next is
    /// its to hand over: the engine's appends to the process that drives the
    /// run, and the log past the seed's last seq to a process that follows it.
    pub(crate) fn open(env: SurfaceEnv<'_>, seed: Vec<StoredEvent>) -> Self {
        let draw = match env.delivery {
            Delivery::Quiet => {
                return Self {
                    feed: None,
                    painter: None,
                    curtain: Curtain::none(),
                }
            }
            Delivery::Lines { reason } => Draw::Lines(Lines::open(stderr(), reason)),
            Delivery::Live => {
                let screen = Screen::watched(Term::buffered_stderr());
                match Region::open(screen, env.glyphs, crate::render::stderr_paint(), stderr()) {
                    Ok(region) => Draw::Live(Box::new(
                        region
                            .moving(TerminalEnv::settled().motion)
                            .ringing(env.manifest.config.terminal_bell()),
                    )),
                    Err(_) => Draw::Lines(Lines::open(stderr(), ROW_TEMPLATE)),
                }
            }
        };
        let (feed, beats) = Feed::open();
        let (curtain, standby) = Curtain::open();
        let painter = Painter::seeded(&env, draw, seed);
        Self {
            feed: Some(feed),
            painter: Some(tokio::spawn(painter::paint(painter, beats, standby))),
            curtain,
        }
    }

    /// Hands the surface an event read off the log, waiting for room in its
    /// queue rather than dropping it: the log is where a dropped event is
    /// recovered from, so one read from there has no other copy. Answers
    /// whether the painter took it; nothing draws under `--quiet`, and that
    /// is taking it too.
    pub(crate) async fn deliver(&self, event: StoredEvent) -> bool {
        match &self.feed {
            Some(feed) => feed.deliver(event).await,
            None => true,
        }
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

    /// Whether this surface says what happens in the run as it happens. Nothing does under
    /// `--quiet`, so whoever else speaks has to say it itself.
    pub(crate) fn chronicles(&self) -> bool {
        self.feed.is_some()
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
    written: yunta_testkit_core::Captured,
}

#[cfg(test)]
impl Watched {
    /// A terminal `rows` by `columns`, with nothing on it.
    pub(crate) fn sized(rows: u16, columns: u16) -> Self {
        Self {
            term: indicatif::InMemoryTerm::new(rows, columns),
            written: yunta_testkit_core::Captured::default(),
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
        // Opening and closing an update is no row of the transcript a test
        // reads back; a title and a bell still are, so a test that the surface
        // wrote nothing under a prompt still catches them.
        if text == surco::paint::SYNC_BEGIN || text == surco::paint::SYNC_END {
            return;
        }
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
    use surco::motion::Motion;

    fn env(stderr_is_terminal: bool, term: Option<&str>) -> TerminalEnv {
        TerminalEnv {
            stderr_is_terminal,
            term: term.map(str::to_string),
            motion: Motion::Moves,
        }
    }

    #[test]
    fn quiet_settles_the_delivery_whatever_the_terminal_offers() {
        let terminal = env(true, Some("xterm-256color"));
        assert_eq!(Delivery::choose(true, &terminal), Delivery::Quiet);
    }

    #[test]
    fn a_terminal_on_stderr_gets_the_region() {
        let terminal = env(true, Some("xterm-256color"));
        assert_eq!(Delivery::choose(false, &terminal), Delivery::Live);
    }

    #[test]
    fn every_downgrade_names_what_was_missing() {
        for (env, reason) in [
            (env(false, Some("xterm")), NOT_A_TERMINAL),
            (env(true, Some("dumb")), DUMB_TERMINAL),
        ] {
            assert_eq!(Delivery::choose(false, &env), Delivery::Lines { reason });
        }
    }

    #[test]
    fn the_delivery_reads_nothing_about_color() {
        // `NO_COLOR` is the color policy's, never the delivery's: the
        // region's words do not need its color, and a terminal gets it.
        let terminal = env(true, Some("xterm"));
        assert_eq!(Delivery::choose(false, &terminal), Delivery::Live);
    }
}

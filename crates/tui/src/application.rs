//! The application at a terminal: the loop that holds the terminal, reads what arrives,
//! hands it to the application's state, carries out what the state asks through the
//! performer, and draws.
//!
//! It waits on four things at once: a key, a paste or a resize from the terminal; what
//! the performer's work hands back; the time, every [`State::TIME_EVERY`], read from the
//! clock it is handed; and whatever asks the process to end. After each wake it hands in
//! everything already waiting and draws once. What it laid out goes back to the state only
//! when it changed. Each drawing is written whole as one synchronized update, and a
//! drawing that changes no cell writes nothing, so a screen at rest costs the terminal
//! nothing.
//!
//! What only a terminal does is done here: a page shown with what the binary hands in to
//! show one, a failure handed back as [`Input::Unshown`]; Ctrl-Z, the terminal's own key,
//! suspends; leaving ends the loop, and the terminal is given back on every way out. A
//! terminal that fails the application is said apart from how it ended
//! ([`TerminalFailure`]): before it was held, the binary can print instead; after, it can
//! only say so.
//!
//! The loop never asks the terminal anything: a terminal that answers nothing would hold
//! it, then fail it. So the screen is drawn whole after a suspend by laying it out afresh
//! at the terminal's size, not by Ratatui's clearing, which asks where the cursor stands.

use std::collections::VecDeque;
use std::future::Future;
use std::io::{self, stdout, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use surco::components::ScreenLaid;
use surco::look::{Look, Width};
use surco::paint::{SYNC_BEGIN, SYNC_END};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::time::MissedTickBehavior;
use yunta_core::Clock;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

use crate::drawn::drawn;
use crate::full_screen::FullScreen;
use crate::keys;
use crate::layout;

/// The application at a terminal, with what it is handed to run: the performer that
/// carries its work out, the look it draws in, the clock it reads the time from, and what
/// shows a page.
pub struct Application {
    performer: Arc<Performer>,
    look: Look,
    clock: Arc<dyn Clock>,
    show_page: fn(&Path) -> std::io::Result<()>,
}

/// How the application ended: left, interrupted by Ctrl-C, or asked to end by what the
/// binary handed in, with what asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit<E> {
    Left,
    Interrupted,
    AskedToEnd(E),
}

/// How the terminal failed the application, with what it said. Not how the application
/// ended ([`Exit`]): the person or the process ends it; this, the terminal.
#[derive(Debug)]
pub enum TerminalFailure {
    /// The terminal would not be held: it refused raw mode or its other screen, or did not
    /// say its size. Nothing was drawn, so the binary can print as off a terminal.
    Unheld(io::Error),
    /// A write, a read or a stop failed once the application was open. The terminal was
    /// given back.
    Lost(io::Error),
}

impl std::fmt::Display for TerminalFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalFailure::Unheld(error) => write!(f, "the terminal would not be held: {error}"),
            TerminalFailure::Lost(error) => {
                write!(
                    f,
                    "the terminal failed while the application was open: {error}"
                )
            }
        }
    }
}

impl std::error::Error for TerminalFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TerminalFailure::Unheld(error) | TerminalFailure::Lost(error) => Some(error),
        }
    }
}

impl Application {
    pub fn new(
        performer: Arc<Performer>,
        look: Look,
        clock: Arc<dyn Clock>,
        show_page: fn(&Path) -> std::io::Result<()>,
    ) -> Self {
        Application {
            performer,
            look,
            clock,
            show_page,
        }
    }

    /// Holds the terminal and runs the application `opened`, with what it asks as it
    /// opens, until the person leaves or `asked_to_end` resolves; the terminal is given
    /// back on every way out.
    pub async fn run<E>(
        &self,
        opened: (State, Vec<Request>),
        asked_to_end: impl Future<Output = E>,
    ) -> Result<Exit<E>, TerminalFailure> {
        let full_screen = FullScreen::held().map_err(TerminalFailure::Unheld)?;
        let drawing = Synchronized::default();
        let terminal = ratatui::Terminal::new(CrosstermBackend::new(drawing.clone()))
            .map_err(TerminalFailure::Unheld)?;
        self.held((full_screen, terminal, drawing), opened, asked_to_end)
            .await
            .map_err(TerminalFailure::Lost)
    }

    /// The loop on the terminal `held`, from the application `opened` until it ends.
    async fn held<E>(
        &self,
        (mut full_screen, mut terminal, drawing): (FullScreen, Drawing, Synchronized),
        opened: (State, Vec<Request>),
        asked_to_end: impl Future<Output = E>,
    ) -> io::Result<Exit<E>> {
        let (inputs, mut answered) = mpsc::unbounded_channel();
        let mut events = EventStream::new();
        let mut cadence = tokio::time::interval(State::TIME_EVERY);
        cadence.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let (mut state, asked) = opened;
        let mut running = Running {
            application: self,
            inputs,
            drawing,
            laid: None,
            on_screen: None,
        };
        if let Some(exit) = running.carried_out(&mut state, asked, false) {
            return Ok(exit);
        }
        tokio::pin!(asked_to_end);
        loop {
            if let Some(exit) = running.draw(&mut terminal, &mut state, &mut stdout())? {
                return Ok(exit);
            }
            let arrived = tokio::select! {
                ended = &mut asked_to_end => return Ok(Exit::AskedToEnd(ended)),
                event = events.next() => match event {
                    Some(Ok(event)) => Arrived::Event(event),
                    Some(Err(error)) => return Err(error),
                    None => return Ok(Exit::Left),
                },
                Some(input) = answered.recv() => Arrived::Input(input),
                _ = cadence.tick() => Arrived::Input(Input::Time(self.clock.now())),
            };
            let mut arrivals = vec![arrived];
            while let Ok(input) = answered.try_recv() {
                arrivals.push(Arrived::Input(input));
            }
            for arrived in arrivals {
                let held = (&mut full_screen, &mut terminal);
                if let Some(exit) = running.arrived(&mut state, arrived, held)? {
                    return Ok(exit);
                }
            }
        }
    }
}

/// What draws on the terminal: Ratatui's, writing into a drawing held until it ends.
type Drawing = ratatui::Terminal<CrosstermBackend<Synchronized>>;

/// The terminal while the application runs: what holds it, and what draws on it.
type Held<'a> = (&'a mut FullScreen, &'a mut Drawing);

/// What woke the loop: something the terminal said, or an input for the state.
enum Arrived {
    Event(Event),
    Input(Input),
}

/// The application while it runs: where the performer hands its answers, what a drawing
/// writes, what was last laid out of the screen, and the cells last written to the
/// terminal, which a drawing that changes none of them leaves as they are.
struct Running<'a> {
    application: &'a Application,
    inputs: UnboundedSender<Input>,
    drawing: Synchronized,
    laid: Option<ScreenLaid>,
    on_screen: Option<Buffer>,
}

impl Running<'_> {
    /// What `arrived` does: a key or a paste handed in as strokes, Ctrl-Z suspending with
    /// the screen drawn whole after it, an input handed in; with each request carried out.
    /// How the application ended, where it did.
    fn arrived<E>(
        &mut self,
        state: &mut State,
        arrived: Arrived,
        (full_screen, terminal): Held<'_>,
    ) -> io::Result<Option<Exit<E>>> {
        let strokes = match arrived {
            Arrived::Input(input) => {
                let asked = state.update(input);
                return Ok(self.carried_out(state, asked, false));
            }
            Arrived::Event(Event::Key(key)) if keys::suspends(key) => {
                full_screen.suspend()?;
                drawn_afresh(terminal)?;
                // The terminal cleared: the next drawing is written whole.
                self.on_screen = None;
                return Ok(None);
            }
            Arrived::Event(Event::Key(key)) => keys::stroke(key).into_iter().collect(),
            Arrived::Event(Event::Paste(text)) => keys::pasted(&text, state.typing()),
            Arrived::Event(_) => Vec::new(),
        };
        for stroke in strokes {
            let asked = state.update(Input::Stroke(stroke));
            let interrupted = stroke == Stroke::Interrupt;
            if let Some(exit) = self.carried_out(state, asked, interrupted) {
                return Ok(Some(exit));
            }
        }
        Ok(None)
    }

    /// Each of `asked` carried out through the performer in the order it was asked, a page
    /// shown and a failure to show it handed back, what that asks carried out after; how the
    /// application ended where one asked it to leave, after a Ctrl-C where it was
    /// `interrupted`. Leaving comes last: what the state asks before it, such as letting go
    /// of the work under way, is carried out first.
    fn carried_out<E>(
        &self,
        state: &mut State,
        asked: Vec<Request>,
        interrupted: bool,
    ) -> Option<Exit<E>> {
        let mut waiting = VecDeque::from(asked);
        while let Some(request) = waiting.pop_front() {
            match self.application.performer.take(request, &self.inputs) {
                Some(Request::Leave) => {
                    return Some(match interrupted {
                        true => Exit::Interrupted,
                        false => Exit::Left,
                    })
                }
                Some(Request::ShowPage(page)) => {
                    if (self.application.show_page)(&page).is_err() {
                        waiting.extend(state.update(Input::Unshown(page)));
                    }
                }
                Some(_) | None => {}
            }
        }
        None
    }

    /// The state's screen drawn and written to `out` as one update, or nothing where it
    /// changes no cell of what the terminal shows; and what was laid out of it handed back
    /// where it changed, with what that asks carried out: how the application ended, where
    /// that asked it to leave.
    fn draw<E>(
        &mut self,
        terminal: &mut Drawing,
        state: &mut State,
        out: &mut impl Write,
    ) -> io::Result<Option<Exit<E>>> {
        let look = self.application.look;
        let mut laid = ScreenLaid::default();
        let completed = terminal.draw(|frame| {
            let columns = usize::from(layout::inner_columns(frame.area().width));
            let screen = state.screen(&Look {
                width: Width::exact(columns),
                ..look
            });
            laid = drawn(frame, &screen, &look);
        })?;
        let cells = completed.buffer.clone();
        match self.on_screen.as_ref() == Some(&cells) {
            true => self.drawing.discarded(),
            false => {
                self.drawing.written(out)?;
                self.on_screen = Some(cells);
            }
        }
        if self.laid == Some(laid) {
            return Ok(None);
        }
        self.laid = Some(laid);
        let asked = state.update(Input::Laid(laid));
        Ok(self.carried_out(state, asked, false))
    }
}

/// `terminal` cleared and what it drew forgotten, so its next drawing writes every cell:
/// laid out afresh at the size the terminal says, which asks it nothing. Not
/// `Terminal::clear`, which asks the terminal where its cursor stands, to put it back,
/// and waits on the answer; a full screen keeps no cursor of the person's.
fn drawn_afresh(terminal: &mut Drawing) -> io::Result<()> {
    let size = terminal.size()?;
    terminal.resize(Rect::from((Position::ORIGIN, size)))
}

/// What a drawing writes, held until it ends, then written to the terminal whole as one
/// synchronized update, or discarded where the drawing changed no cell. The backend writes
/// into it and the loop writes it out: two handles on one hold.
#[derive(Clone, Default)]
struct Synchronized {
    drawing: Arc<Mutex<Vec<u8>>>,
}

impl Synchronized {
    /// The drawing held, written whole to `out` and the hold emptied.
    fn written(&self, out: &mut impl Write) -> io::Result<()> {
        let mut drawing = self
            .drawing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if drawing.is_empty() {
            return Ok(());
        }
        out.write_all(SYNC_BEGIN.as_bytes())
            .and_then(|()| out.write_all(&drawing))
            .and_then(|()| out.write_all(SYNC_END.as_bytes()))
            .and_then(|()| out.flush())?;
        drawing.clear();
        Ok(())
    }
}

impl Synchronized {
    /// What the drawing held, dropped unwritten.
    fn discarded(&self) {
        self.drawing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

impl Write for Synchronized {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.drawing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Nothing reaches the terminal until the drawing ends.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;

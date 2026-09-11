//! How a person answers a run at a terminal.
//!
//! Two shapes reach a person, and they differ in kind. Almost every
//! blocking decision the product has is one of them — an internal gate,
//! an exhausted re-route, a promotion, an external gate with no
//! reachable forge, a scope expansion, a budget cap, a loop cap — and
//! all of them arrive as the same object: a summary, the evidence the
//! engine attached to it, and options that each declare what they trade
//! off. [`decide`] renders that object, once, for all of them. The
//! other shape is a `kind: questions` artifact, which is a sequence of
//! prompts rather than a menu, and [`answer`] walks it.
//!
//! Escape ends any prompt here with no answer: the run parks with its
//! state intact, which is the gesture for "not me, not now". Ctrl-C is
//! the other one and means the opposite — it stops the run — and
//! reaches the one cancellation bridge from inside a prompt too
//! ([`Console::interrupt`]).

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dialoguer::console::{Key, Term};
use yunta_adapters::signal::{signal_process, Signal};
use yunta_core::{Pid, Responder};

use crate::error::warn;

mod decision;
mod field;
mod form;
mod keys;
mod menu;

pub(crate) use decision::decide;
pub(crate) use form::answer;

/// What Escape does, said wherever a prompt opens: one phrase, so the
/// gesture reads the same on a menu and on a line being typed.
pub(crate) const PARKS: &str = "esc parks the run";

/// What marks a line an answer is given on, typed or echoed back, so a
/// person reads a round as answers under the things that asked for
/// them.
pub(crate) const ANSWER: &str = "> ";

/// A prompt that produced no answer, and why.
///
/// Every variant ends the same way for the engine — nothing is recorded
/// and the run parks — so the reasons are kept apart here and turned
/// into text once, where the caller degrades.
#[derive(Debug)]
pub(crate) enum NoAnswer {
    /// Escape: the person chose not to answer now.
    Declined,
    /// Ctrl-C, already re-raised at this process so the run's own
    /// cancellation is on its way.
    Interrupted,
    /// The list answered with a choice that is not on it.
    OffMenu,
    /// The console could not be read or drawn on.
    Unreadable(std::io::Error),
    /// The thread the prompt ran on ended without producing either.
    Failed(String),
}

impl From<std::io::Error> for NoAnswer {
    /// An interrupted read is Ctrl-C, never a console this surface
    /// could not use: `console` turns the byte into this error kind and
    /// re-raises the signal on its way out, so what reaches here is a
    /// person stopping the run.
    fn from(source: std::io::Error) -> Self {
        match source.kind() {
            std::io::ErrorKind::Interrupted => NoAnswer::Interrupted,
            _ => NoAnswer::Unreadable(source),
        }
    }
}

/// What a prompt returns: the answer, or [`NoAnswer`].
pub(crate) type Answered<T> = Result<T, NoAnswer>;

/// A terminal a person can be prompted on: keystrokes read from stdin,
/// lines drawn on stderr, leaving stdout to whatever the caller pipes.
///
/// Cloning duplicates the handle, never the terminal. What that is for
/// is [`Console::restore`]: the caller keeps a handle of its own, so
/// what a prompt took from the terminal is put back even where the
/// prompt is left mid-read.
#[derive(Clone)]
pub(crate) struct Console {
    term: Term,
    /// Whether a prompt on this terminal has the cursor hidden. Two
    /// paths put it back — the prompt as it ends, and the caller where
    /// the prompt cannot — and this is what keeps them from both doing
    /// it, so what the terminal is handed back is one put-back for one
    /// taking-away.
    hidden: Arc<AtomicBool>,
}

impl Console {
    /// The console this process can prompt on, or `None` when it has
    /// none.
    ///
    /// Both ends are established here because no prompt below checks
    /// them: with stdin redirected, the key reader falls back to
    /// `/dev/tty` and waits forever for a person who is not there; with
    /// stderr redirected, every read answers with a key nobody pressed.
    /// A caller that gets `None` has no surface and degrades to parking
    /// the run.
    ///
    /// Nothing attended at all is the ordinary headless case and passes
    /// without a word. A person at the keyboard whose run has nowhere to
    /// draw is not: the run parks on them, so it says what was missing.
    pub(crate) fn open() -> Option<Self> {
        if !std::io::stdin().is_terminal() {
            return None;
        }
        let term = Term::stderr();
        if !term.is_term() {
            warn(
                "nothing was asked here: a prompt draws on stderr, and this run's is \
                 redirected — the run parks with its state intact; run it again with \
                 stderr on a terminal to be asked",
            );
            return None;
        }
        Some(Self {
            term,
            hidden: Arc::new(AtomicBool::new(false)),
        })
    }

    /// One line, drawn as it is.
    pub(crate) fn say(&self, line: &str) -> std::io::Result<()> {
        self.term.write_line(line)
    }

    /// `text` with every line drawn under `indent`.
    pub(crate) fn block(&self, text: &str, indent: &str) -> std::io::Result<()> {
        self.say(&yunta_core::text::indent(text, indent))
    }

    /// The terminal the prompt libraries draw their own blocks on.
    pub(crate) fn term(&self) -> &Term {
        &self.term
    }

    /// The next keystroke.
    ///
    /// The read puts the terminal in raw mode, which turns off the line
    /// discipline's signal keys, so a typed Ctrl-C arrives here as
    /// [`Key::CtrlC`] instead of reaching this process as a signal.
    /// [`Console::interrupt`] is what puts it back on the one path that
    /// stops a run.
    pub(crate) fn read_key(&self) -> std::io::Result<Key> {
        self.term.read_key_raw()
    }

    /// The cells one row of this terminal holds.
    pub(crate) fn width(&self) -> usize {
        usize::from(self.term.size().1)
    }

    /// Redraws the row a line is being typed on: `prompt`, then `text`,
    /// with the cursor left `back` cells from the end.
    ///
    /// One row, drawn over itself: the clearing and the cursor move both
    /// act on the row the cursor is on, so `prompt` and `text` together
    /// have to fit one. [`Console::width`] is what says how much that
    /// is.
    pub(crate) fn draw_line(&self, prompt: &str, text: &str, back: usize) -> std::io::Result<()> {
        self.term.clear_line()?;
        self.term.write_str(prompt)?;
        self.term.write_str(text)?;
        self.term.move_cursor_left(back)?;
        self.term.flush()
    }

    /// Finishes the row being typed on: what it was showing gives way to
    /// `line`, and the cursor moves past it.
    ///
    /// Written as a line rather than redrawn in place, so an answer
    /// wider than the terminal is left on screen whole — laid out by the
    /// terminal over as many rows as it takes — rather than as the part
    /// of it the editing row had room for.
    pub(crate) fn end_line(&self, line: &str) -> std::io::Result<()> {
        self.term.clear_line()?;
        self.term.write_line(line)
    }

    /// Records that a prompt on this terminal has taken the cursor
    /// away, so [`Console::restore`] knows there is one to put back.
    pub(crate) fn hiding(&self) {
        self.hidden.store(true, Ordering::Release);
    }

    /// Puts back what a prompt takes from this terminal while it is
    /// open: the cursor a list hides for as long as it is drawing.
    ///
    /// A prompt does this for itself as it ends, whichever way it ends.
    /// The caller does it for the one ending a prompt cannot reach — a
    /// read abandoned mid-key, where the thread that would have put the
    /// cursor back is still waiting on a key nobody is going to press.
    /// Leaving it hidden hands the shell this run returns to a terminal
    /// with no cursor in it, which nothing that runs next puts back.
    ///
    /// Whichever of the two arrives first does it, and only that one: a
    /// person stopping a prompt sets both off at once, and a terminal
    /// told twice to show a cursor it is already showing is a terminal
    /// nobody can read a run's drawing off.
    pub(crate) fn restore(&self) {
        if !self.hidden.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Err(e) = self.term.show_cursor() {
            warn(format!("could not put the terminal's cursor back: {e}"));
        }
    }

    /// Sends this process the interrupt the terminal did not.
    ///
    /// A typed Ctrl-C then stops the run through the same bridge a
    /// signal from outside takes — one source of truth about whether a
    /// person stopped the run, rather than a second decision taken at
    /// the prompt.
    pub(crate) fn interrupt(&self) {
        if let Err(e) = signal_process(Pid::current(), Signal::SIGINT) {
            warn(format!("could not stop the run from this prompt: {e}"));
        }
    }
}

/// Names the identity the answer is about to be recorded under, and
/// returns it.
///
/// Nothing at a console prompt authenticates who is typing, so what the
/// log gets is the shell's own ambient identity, marked unverified.
/// Saying so at the moment it is recorded is what keeps an unsigned
/// decision from reading like a signed one.
fn attributed(console: &Console) -> std::io::Result<Responder> {
    let responder = crate::identity::responder(None);
    console.say(&format!(
        "attributed to `{responder}` — unsigned, nothing here authenticates who typed"
    ))?;
    Ok(responder)
}

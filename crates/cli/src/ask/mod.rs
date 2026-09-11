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
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dialoguer::console::{Key, Term};
use nix::sys::termios::{tcgetattr, tcsetattr, SetArg, Termios};
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

/// The console the last prompt opened on this process's terminal, or
/// `None` in a process where no prompt opened one.
///
/// A key read turns the line discipline off for as long as it reads and
/// puts it back between reads, so the thread left behind by a read
/// nobody is coming to finish takes it again a moment after anyone else
/// hands it over. Nothing runs after that thread except the process
/// leaving, which is why what a prompt took is kept here and handed
/// back by [`restore_terminal`] rather than by whoever stopped waiting
/// on the prompt.
///
/// One console, not a list: prompts open one at a time, each one ends
/// before the next opens, and the terminal the last of them left is the
/// terminal this process leaves behind.
static PROMPTED_ON: Mutex<Option<Console>> = Mutex::new(None);

/// Hands the terminal back as this process leaves: the line discipline
/// a key read turned off, and the cursor a list hid.
///
/// A process that prompted nobody has nothing to hand back and touches
/// nothing — a command with no question to ask never opens a console,
/// so the terminal it was handed is the terminal it leaves. Handing
/// back happens once: what is taken here is gone, so a second call is
/// no second put-back.
pub(crate) fn restore_terminal() {
    if let Some(console) = prompted_on().take() {
        console.restore();
    }
}

/// The keeper, readable through a lock a panicking prompt cannot take
/// away: a terminal left in raw mode is worse than a terminal handed
/// back from behind a poisoned lock.
fn prompted_on() -> MutexGuard<'static, Option<Console>> {
    PROMPTED_ON.lock().unwrap_or_else(PoisonError::into_inner)
}

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
/// is [`restore_terminal`]: the process keeps a handle of its own, so
/// what a prompt took from the terminal is put back even where the
/// prompt is left mid-read.
#[derive(Clone)]
pub(crate) struct Console {
    term: Term,
    /// The line discipline the terminal was handed over in — what every
    /// key read turns off for as long as it reads, and what
    /// [`Console::restore`] means by putting it back. `None` where the
    /// terminal would not say what mode it is in.
    mode: Option<Termios>,
    /// Whether a prompt on this terminal has the cursor hidden. Two
    /// paths put it back — the prompt as it ends, and the process where
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
        let console = Self {
            term,
            mode: handed_mode(),
            hidden: Arc::new(AtomicBool::new(false)),
        };
        *prompted_on() = Some(console.clone());
        Some(console)
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
    /// open: the line discipline every key read turns off, and the
    /// cursor a list hides for as long as it is drawing.
    ///
    /// A prompt does this for itself as it ends, whichever way it ends.
    /// The process does it for the one ending a prompt cannot reach — a
    /// read abandoned mid-key, where the thread that would have put
    /// them back is still waiting on a key nobody is going to press.
    /// Left as that read left it, the shell this run returns to echoes
    /// nothing a person types into it, shows no cursor to type at, and
    /// answers no Enter, and nothing that runs next puts any of it
    /// back. The process and not whoever stopped waiting on the prompt,
    /// because that thread turns raw mode back on between its own
    /// reads: anything that hands the terminal over while it still runs
    /// is handing over a terminal it takes again.
    ///
    /// The two are put back on different terms because they are taken
    /// on different terms. Setting the terminal to the mode it was
    /// handed is what "as it was found" means, and a terminal already
    /// in that mode is unchanged by being told so, which is why both
    /// paths may do it. Showing the cursor is a sequence written to the
    /// terminal, and a second one is a second cursor as far as anything
    /// reading that terminal can tell: whichever of the two paths
    /// arrives first does that, and only that one.
    pub(crate) fn restore(&self) {
        if let Some(mode) = &self.mode {
            // Now rather than once the output drains: the read this is
            // putting the terminal back for is one nobody is coming to
            // finish.
            if let Err(e) = tcsetattr(std::io::stdin(), SetArg::TCSANOW, mode) {
                warn(format!(
                    "could not put this terminal back to reading a line at a time: {e} — \
                     run `stty sane` to type into your shell again"
                ));
            }
        }
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

/// The line discipline stdin is in as this console opens — what a key
/// read turns off and what [`Console::restore`] puts back.
///
/// A terminal that will not say what mode it is in is one whose mode
/// cannot be put back either, so the reader is told once, here, rather
/// than left to find it out in the shell this run returns to.
fn handed_mode() -> Option<Termios> {
    match tcgetattr(std::io::stdin()) {
        Ok(mode) => Some(mode),
        Err(e) => {
            warn(format!(
                "this terminal does not say what mode it is in ({e}) — a prompt this run \
                 leaves mid-read leaves it as the read left it; run `stty sane` to type \
                 into your shell again"
            ));
            None
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A handle on this process's stderr with no line discipline to put
    /// back: what these tests read is which console the process hands
    /// back, never what handing one back does to a terminal.
    fn console() -> Console {
        Console {
            term: Term::stderr(),
            mode: None,
            hidden: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn the_console_a_prompt_opened_is_handed_back_by_the_process_exactly_once() {
        let console = console();
        console.hiding();
        *prompted_on() = Some(console.clone());

        restore_terminal();
        assert!(
            !console.hidden.load(Ordering::Acquire),
            "the cursor a prompt hid is put back as the process leaves"
        );
        assert!(
            prompted_on().is_none(),
            "what was handed back is gone, so nothing hands it back a second time"
        );
    }
}

//! How a person answers a run at a terminal.
//!
//! Two shapes reach a person, and they differ in kind. Almost every
//! blocking decision the product has is one of them — an internal gate,
//! an exhausted reroute, a promotion, an external gate with no
//! reachable forge, a scope expansion, a budget cap, a loop cap — and
//! all of them arrive as the same object: a summary, the evidence the
//! engine attached to it, and options that each declare what they trade
//! off. [`decide`] renders that object, once, for all of them. The
//! other shape is a `kind: questions` artifact, which is a round of
//! questions rather than a menu, and [`answer`] holds it in one place.
//!
//! Escape ends any prompt here with no answer: the run parks with its
//! state intact, which is the gesture for "not me, not now". Ctrl-C is
//! the other one and means the opposite — it stops the run — and
//! reaches the one cancellation bridge from inside a prompt too
//! ([`Console::interrupt`]).

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use console::Term;
use nix::sys::termios::{tcgetattr, tcsetattr, SetArg, Termios};
use surco::paint::{
    ALT_SCREEN_OFF, ALT_SCREEN_ON, ALT_SCROLL_OFF, ALT_SCROLL_ON, HIDE_CURSOR, HOME, SHOW_CURSOR,
};
use yunta_core::process::signal::{signal_process, Signal};
use yunta_core::{Env, Pid, Responder};
use yunta_render::prompt::Escape;

use crate::output::warn;
use crate::surface::{Diagnostics, TerminalEnv};

/// Warns that `--interactive` has no terminal to ask on, before the `fallback` the
/// command takes instead.
pub(crate) fn no_terminal(fallback: &str) {
    warn(yunta_render::prompt::no_terminal(fallback));
}

mod choose;
mod decision;
mod held;
mod keyboard;
mod keys;
mod line;
mod round;

pub(crate) use choose::choose;
pub(crate) use decision::{decide, Ask};
pub(crate) use line::ask_line;
pub(crate) use round::answer;

/// The console the last prompt opened on this process's terminal, or
/// `None` in a process where no prompt opened one.
///
/// One console, not a list: prompts open one at a time, each one ends
/// before the next opens, and the terminal the last of them left is the
/// terminal this process leaves behind.
///
/// A prompt holds the terminal in its own mode until it ends, and every
/// ending it can reach puts that back. A panic is the one it cannot, and
/// nothing runs after that thread except the process leaving, which is
/// why what a prompt took is kept here and handed back by
/// [`restore_terminal`].
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
    /// What Escape ends a prompt on this console with — the console
    /// knows, because whoever opened it knows whether a run is behind
    /// it, and every prompt then says the same thing without being
    /// told.
    escape: Escape,
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
    /// Whether a prompt on this terminal is on its other screen, for the same reason:
    /// one way back for one way there.
    alternate: Arc<AtomicBool>,
    /// Whether the run this console asks for says what was answered and by whom, in its own
    /// history. Nobody else does at `resolve-gate` or under `--quiet`, where the console says it.
    chronicled: bool,
    /// Told by whoever stopped waiting on a prompt: the prompt hears it
    /// within a tick of its read, takes what it drew off the screen and
    /// hands the terminal back, before anything else is drawn.
    stopped: Arc<AtomicBool>,
}

impl Console {
    /// Whether a prompt here reaches a person: stdin and stderr both on
    /// a terminal, which is what [`Console::open`] asks before it opens.
    pub(crate) fn attended() -> bool {
        std::io::stdin().is_terminal()
            && Term::stderr().is_term()
            && !TerminalEnv::settled().is_dumb()
    }

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
    /// without a word. A terminal that declares itself dumb is headless
    /// too: it has said it draws nothing beyond text, so nobody is asked
    /// there and the run parks, saying the command for each option. A
    /// person at the keyboard whose run has nowhere to draw is not: the
    /// run parks on them, so it says what was missing.
    ///
    /// What it has to say goes out through `diagnostics`, the run's own
    /// door, rather than straight to stderr: a line written around a
    /// pinned region lands inside the rows the region is redrawing, and
    /// this is said while one may still be up. A command with no run to
    /// draw — `yunta init`, `yunta new` — passes
    /// [`Diagnostics::none`](crate::surface::Diagnostics::none), which
    /// is the same door onto nothing.
    pub(crate) async fn open(diagnostics: &Diagnostics, escape: Escape) -> Option<Self> {
        if TerminalEnv::settled().is_dumb() {
            return None;
        }
        if !std::io::stdin().is_terminal() {
            return None;
        }
        let term = Term::stderr();
        if !term.is_term() {
            diagnostics
                .raise(format!(
                    "{}: nothing was asked here, since a prompt draws on stderr and this run's is \
                     redirected: the run parks with its state intact; run it again with \
                     stderr on a terminal to be asked",
                    crate::error::warning_word()
                ))
                .await;
            return None;
        }
        let console = Self {
            term,
            escape,
            mode: handed_mode(diagnostics).await,
            hidden: Arc::new(AtomicBool::new(false)),
            alternate: Arc::new(AtomicBool::new(false)),
            chronicled: false,
            stopped: Arc::new(AtomicBool::new(false)),
        };
        *prompted_on() = Some(console.clone());
        Some(console)
    }

    /// What Escape ends a prompt on this console with, for the line
    /// every prompt names the keys on.
    pub(crate) fn escape(&self) -> Escape {
        self.escape
    }

    /// This console, for a run that does or does not say what was answered on it.
    pub(crate) fn chronicled_by_the_run(mut self, chronicled: bool) -> Self {
        self.chronicled = chronicled;
        self
    }

    /// One line, drawn as it is.
    pub(crate) fn say(&self, line: &str) -> std::io::Result<()> {
        self.term.write_line(line)
    }

    /// Says `line` unless the run says the same thing in its own history a moment later.
    pub(crate) fn say_unless_chronicled(&self, line: &str) -> std::io::Result<()> {
        match self.chronicled {
            true => Ok(()),
            false => self.say(line),
        }
    }

    /// The terminal the prompt libraries draw their own blocks on.
    pub(crate) fn term(&self) -> &Term {
        &self.term
    }

    /// How a line drawn on this terminal looks: the paint stderr gets,
    /// and rows no wider than the terminal's edge, since a prompt redraws
    /// its rows in place.
    pub(crate) fn look(&self) -> surco::look::Look {
        surco::look::Look {
            glyphs: crate::output::glyphs(),
            paint: crate::output::stderr_paint(),
            width: crate::output::row_width(usize::from(self.term.size().1)),
        }
    }

    /// Hides the cursor, once, so [`Console::restore`] knows there is one to put back.
    pub(crate) fn hide_cursor(&self) -> std::io::Result<()> {
        if !self.hidden.swap(true, Ordering::AcqRel) {
            self.term.write_str(HIDE_CURSOR)?;
        }
        Ok(())
    }

    /// Shows the cursor again, when it is hidden: one put-back for one taking-away.
    pub(crate) fn show_cursor(&self) -> std::io::Result<()> {
        if self.hidden.swap(false, Ordering::AcqRel) {
            self.term.write_str(SHOW_CURSOR)?;
        }
        Ok(())
    }

    /// Puts the terminal on its other screen, once, with the cursor at its first cell and
    /// the wheel sending arrows.
    pub(crate) fn alternate_on(&self) -> std::io::Result<()> {
        if !self.alternate.swap(true, Ordering::AcqRel) {
            self.term
                .write_str(&format!("{ALT_SCREEN_ON}{ALT_SCROLL_ON}{HOME}"))?;
            self.term.flush()?;
        }
        Ok(())
    }

    /// Comes back from the other screen, to what was on this one.
    pub(crate) fn alternate_off(&self) -> std::io::Result<()> {
        if self.alternate.swap(false, Ordering::AcqRel) {
            self.term
                .write_str(&format!("{ALT_SCROLL_OFF}{ALT_SCREEN_OFF}"))?;
            self.term.flush()?;
        }
        Ok(())
    }

    /// Puts back what a prompt takes from this terminal while it is
    /// open: the line discipline every key read turns off, and the
    /// cursor a list hides for as long as it is drawing.
    ///
    /// A prompt does this for itself as it ends, whichever way it ends.
    /// The process does it for the one ending a prompt cannot reach
    /// itself — a panic, where the prompt's own code never gets to put
    /// them back. Left as the panic left it, the shell this run returns
    /// to echoes nothing a person types into it, shows no cursor to
    /// type at, and answers no Enter, and nothing that runs next puts
    /// any of it back. The process and not whoever ended the prompt,
    /// because a read that is still running turns raw mode back on
    /// between its own reads: anything that hands the terminal over
    /// while it still runs is handing over a terminal it takes again.
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
                    "could not put this terminal back to reading a line at a time: {e}; \
                     run `stty sane` to type into your shell again"
                ));
            }
        }
        if let Err(e) = self.alternate_off() {
            warn(format!(
                "could not come back from the terminal's other screen: {e}"
            ));
        }
        if let Err(e) = self.show_cursor() {
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
async fn handed_mode(diagnostics: &Diagnostics) -> Option<Termios> {
    match tcgetattr(std::io::stdin()) {
        Ok(mode) => Some(mode),
        Err(e) => {
            diagnostics
                .raise(format!(
                    "{}: this terminal does not say what mode it is in ({e}); a prompt this \
                     run leaves mid-read leaves it as the read left it; run `stty sane` to \
                     type into your shell again",
                    crate::error::warning_word()
                ))
                .await;
            None
        }
    }
}

/// Names the identity the answer is about to be recorded under, and
/// returns it.
///
/// Whom an answer is recorded as: nothing at a console prompt authenticates who is
/// typing, so the log gets the shell's own ambient identity, marked unverified, and
/// says so. A run's own history says it in the identity it records, marked
/// `unverified:`, so a console whose run keeps one leaves it to that.
fn attributed(console: &Console, env: &Env) -> std::io::Result<Responder> {
    let responder = Responder::answering(None, env.user.as_deref());
    console.say_unless_chronicled(&yunta_render::prompt::attributed(&responder))?;
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
            escape: Escape::Parks,
            mode: None,
            hidden: Arc::new(AtomicBool::new(false)),
            alternate: Arc::new(AtomicBool::new(false)),
            chronicled: false,
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn the_console_a_prompt_opened_is_handed_back_by_the_process_exactly_once() {
        let console = console();
        console
            .hide_cursor()
            .expect("a test's stderr takes the cursor's hiding");
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

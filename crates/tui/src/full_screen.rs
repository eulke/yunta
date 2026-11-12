//! The terminal held while the application is open, and given back on every way out.
//!
//! Held, the terminal is on its other screen, in raw mode, marking what is pasted into it,
//! its cursor hidden, and its wheel sending arrows on the other screen, as the CLI's
//! desk has it; the mouse is never captured, so a person still selects and copies.
//!
//! It is given back as it was found: when the holding is dropped; by a panic hook
//! installed while it is held, before the panic is said, which is the only way back where
//! a release build aborts on a panic and no drop runs; and before the process stops on
//! Ctrl-Z, held again when it goes on. Every step of giving it back is taken whatever the
//! step before came to. What the terminal is told is Surco's escapes, as the CLI tells it;
//! crossterm puts it in raw mode and back.
//!
//! Under a shell's job control the process stops on `SIGTSTP` until `fg` sends it on. In a
//! process group no shell controls the kernel discards the signal, and the application goes
//! on at once, holding the terminal again. A test runs in either, so it sends what `fg`
//! sends while it waits.

use std::io::{stdout, Write};
use std::panic::{self, PanicHookInfo};
use std::sync::Arc;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use nix::sys::signal::{raise, Signal};
use surco::paint::{
    ALT_SCREEN_OFF, ALT_SCREEN_ON, ALT_SCROLL_OFF, ALT_SCROLL_ON, HIDE_CURSOR, PASTE_OFF, PASTE_ON,
    SHOW_CURSOR,
};

/// What the process did with a panic before the terminal was held, which the hook the
/// holding installs says the panic through, and which is put back when it is given back.
type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>;

/// The terminal held. Not the terminal Ratatui draws on: what this process took from the
/// person's terminal, and owes back.
pub(crate) struct FullScreen {
    /// The panic hook the holding found, put back when the holding is dropped.
    found: Option<Arc<PanicHook>>,
}

impl FullScreen {
    /// The terminal held, with the panic hook that gives it back installed.
    pub(crate) fn held() -> std::io::Result<FullScreen> {
        hold()?;
        let found: Arc<PanicHook> = Arc::new(panic::take_hook());
        let said = Arc::clone(&found);
        panic::set_hook(Box::new(move |info| {
            // A panic is being said: a terminal that cannot be given back adds nothing to it.
            // best-effort: the panic is said whatever the terminal comes to.
            let _ = give_back();
            said(info);
        }));
        Ok(FullScreen { found: Some(found) })
    }

    /// The process stopped as a terminal's Ctrl-Z stops it, the terminal given back while
    /// it stands and held again once it goes on. The screen is drawn whole after it.
    pub(crate) fn suspend(&mut self) -> std::io::Result<()> {
        give_back()?;
        raise(Signal::SIGTSTP)?;
        hold()
    }
}

impl Drop for FullScreen {
    fn drop(&mut self) {
        // best-effort: a terminal that cannot be given back has nobody left to tell.
        let _ = give_back();
        // A thread a panic unwinds through may not touch the panic hook: the hook the
        // holding installed stays, giving back a terminal already given back, then saying
        // any later panic as the hook it found does.
        if std::thread::panicking() {
            return;
        }
        if let Some(found) = self.found.take() {
            drop(panic::take_hook());
            panic::set_hook(Box::new(move |info| found(info)));
        }
    }
}

/// Takes the terminal: raw mode, the other screen, pastes marked, the cursor hidden, the
/// wheel sending arrows. What was taken is given back where a step fails.
fn hold() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    let held = [ALT_SCREEN_ON, PASTE_ON, HIDE_CURSOR, ALT_SCROLL_ON]
        .into_iter()
        .try_for_each(|escape| out.write_all(escape.as_bytes()))
        .and_then(|()| out.flush());
    if let Err(refused) = held {
        // The terminal refused to be held: what it took is given back, and why it refused
        // is what is said.
        // best-effort: the refusal is the error this returns.
        let _ = give_back();
        return Err(refused);
    }
    Ok(())
}

/// Gives the terminal back as it was found, each step taken whatever the one before came
/// to; the first that failed.
fn give_back() -> std::io::Result<()> {
    let mut out = stdout();
    let steps = [
        out.write_all(ALT_SCROLL_OFF.as_bytes()),
        out.write_all(PASTE_OFF.as_bytes()),
        out.write_all(SHOW_CURSOR.as_bytes()),
        out.write_all(ALT_SCREEN_OFF.as_bytes()),
        out.flush(),
        disable_raw_mode(),
    ];
    steps.into_iter().collect()
}

#[cfg(test)]
mod tests;

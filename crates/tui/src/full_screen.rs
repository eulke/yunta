//! The terminal held while the application is open, and given back on every way out.
//!
//! Held, the terminal is on its other screen, in raw mode, marking what is pasted into it,
//! its cursor hidden, and its wheel sending arrows on the other screen, as the CLI's
//! desk has it; the mouse is never captured, so a person still selects and copies.
//!
//! A title the application gives the window while it is held (`WindowTitle`) keeps the
//! window's own under it, which the terminal is told to give back with the rest; a changed
//! title gives the one before back first, so the window's own is all a terminal keeps, and
//! it is given back once.
//!
//! It is given back as it was found: when the holding is dropped; by a panic hook
//! installed while it is held, before the panic is said, which is the only way back where
//! a release build aborts on a panic and no drop runs; and before the process stops on
//! Ctrl-Z, held again when it goes on, where the next drawing gives the window its title
//! again. Every step of giving it back is taken whatever the step before came to. What the
//! terminal is told is Surco's escapes, as the CLI tells it; crossterm puts it in raw mode
//! and back.
//!
//! Under a shell's job control the process stops on `SIGTSTP` until `fg` sends it on. In a
//! process group no shell controls the kernel discards the signal, and the application goes
//! on at once, holding the terminal again. A test runs in either, so it sends what `fg`
//! sends while it waits.

use std::io::{stdout, Write};
use std::panic::{self, PanicHookInfo};
use std::sync::{Arc, Mutex, PoisonError};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use nix::sys::signal::{raise, Signal};
use surco::paint::{
    title_saved, ALT_SCREEN_OFF, ALT_SCREEN_ON, ALT_SCROLL_OFF, ALT_SCROLL_ON, HIDE_CURSOR,
    PASTE_OFF, PASTE_ON, SHOW_CURSOR, TITLE_RESTORED,
};

/// What the process did with a panic before the terminal was held, which the hook the
/// holding installs says the panic through, and which is put back when it is given back.
type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>;

/// The terminal held. Not the terminal Ratatui draws on: what this process took from the
/// person's terminal, and owes back.
pub(crate) struct FullScreen {
    /// The panic hook the holding found, put back when the holding is dropped.
    found: Option<Arc<PanicHook>>,
    /// The title the application gave the window, given back with the terminal.
    title: WindowTitle,
}

impl FullScreen {
    /// The terminal held, with the panic hook that gives it back installed.
    pub(crate) fn held() -> std::io::Result<FullScreen> {
        let title = WindowTitle::default();
        hold(&title)?;
        let found: Arc<PanicHook> = Arc::new(panic::take_hook());
        let (said, owed) = (Arc::clone(&found), title.clone());
        panic::set_hook(Box::new(move |info| {
            // A panic is being said: a terminal that cannot be given back adds nothing to it.
            // best-effort: the panic is said whatever the terminal comes to.
            let _ = give_back(&owed);
            said(info);
        }));
        Ok(FullScreen {
            found: Some(found),
            title,
        })
    }

    /// The window's title as this holding keeps it, which the loop gives the window as each
    /// screen asks and the holding gives back.
    pub(crate) fn window_title(&self) -> WindowTitle {
        self.title.clone()
    }

    /// The process stopped as a terminal's Ctrl-Z stops it, the terminal given back while
    /// it stands and held again once it goes on. The screen is drawn whole after it.
    pub(crate) fn suspend(&mut self) -> std::io::Result<()> {
        give_back(&self.title)?;
        raise(Signal::SIGTSTP)?;
        hold(&self.title)
    }
}

impl Drop for FullScreen {
    fn drop(&mut self) {
        // best-effort: a terminal that cannot be given back has nobody left to tell.
        let _ = give_back(&self.title);
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

/// The title the application gave the window while the terminal is held, the window's own
/// kept under it: shared by the holding, the panic hook it installs and the loop that gives
/// the window each screen's title. Not a screen's title: what the terminal was told of one.
#[derive(Clone, Default)]
pub(crate) struct WindowTitle {
    /// The title the window was given, none while it shows its own.
    given: Arc<Mutex<Option<String>>>,
}

impl WindowTitle {
    /// The window given `title`, told to `out` where it was not given it already: a first
    /// title keeps the window's own, a changed one gives the one before back first, and
    /// none gives the window's own back.
    pub(crate) fn give(&self, title: Option<&str>, out: &mut impl Write) -> std::io::Result<()> {
        let told = {
            let mut given = self.given.lock().unwrap_or_else(PoisonError::into_inner);
            if given.as_deref() == title {
                return Ok(());
            }
            let back = match given.is_some() {
                true => TITLE_RESTORED,
                false => "",
            };
            *given = title.map(str::to_string);
            format!("{back}{}", title.map(title_saved).unwrap_or_default())
        };
        out.write_all(told.as_bytes())?;
        out.flush()
    }

    /// The window's own title given back to `out`, where the window was given another.
    fn given_back(&self, out: &mut impl Write) -> std::io::Result<()> {
        let given = self
            .given
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match given {
            Some(_) => out.write_all(TITLE_RESTORED.as_bytes()),
            None => Ok(()),
        }
    }
}

/// Takes the terminal: raw mode, the other screen, pastes marked, the cursor hidden, the
/// wheel sending arrows. What was taken is given back where a step fails, `title` with it.
fn hold(title: &WindowTitle) -> std::io::Result<()> {
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
        let _ = give_back(title);
        return Err(refused);
    }
    Ok(())
}

/// Gives the terminal back as it was found, the window's own title with it where `title`
/// was given, each step taken whatever the one before came to; the first that failed.
fn give_back(title: &WindowTitle) -> std::io::Result<()> {
    let mut out = stdout();
    let steps = [
        out.write_all(ALT_SCROLL_OFF.as_bytes()),
        out.write_all(PASTE_OFF.as_bytes()),
        out.write_all(SHOW_CURSOR.as_bytes()),
        title.given_back(&mut out),
        out.write_all(ALT_SCREEN_OFF.as_bytes()),
        out.flush(),
        disable_raw_mode(),
    ];
    steps.into_iter().collect()
}

#[cfg(test)]
mod tests;

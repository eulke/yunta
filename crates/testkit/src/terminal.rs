//! A `yunta` run driven on a real terminal.
//!
//! Some behaviors exist nowhere else, and a pipe cannot stand in for
//! any of them: a pasted line break is the byte a typed Enter is, an
//! arrow is an escape sequence and not a character, the read that waits
//! for either holds the terminal in raw mode, and the live region pins
//! itself only where there is a terminal to pin it to. A pty is what
//! puts those under a test — the child's stdio is the slave end, so
//! `is_terminal()` holds at both ends, and the test reads what was
//! drawn and types what a person would from the master end.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, LocalFlags, SetArg};
use nix::unistd::Pid;

use surco::paint::{ALT_SCREEN_OFF, ALT_SCREEN_ON, HIDE_CURSOR, SHOW_CURSOR};

use crate::wait::{wait_for, wait_until, WAIT_DEADLINE};

pub(crate) mod screen;

use screen::{unpainted, Screen};

/// What a run writes to clear a row, which this harness reads a drawing by.
const CLEAR_ROW: &str = "\u{1b}[2K";

/// A run under a terminal of a stated size.
///
/// The size is stated rather than inherited so a list, a region row and
/// anything else measured in cells has the same room in every
/// environment the suite runs in.
pub struct Terminal {
    child: Child,
    keyboard: std::fs::File,
    drawn: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
    /// How the run ended, once it has.
    ended: Option<ExitStatus>,
    /// The cells one row of this terminal holds, as stated when it was opened.
    columns: u16,
    /// The rows this terminal holds, as stated when it was opened.
    rows: u16,
}

impl Terminal {
    /// The cells one row of this terminal holds.
    pub const COLUMNS: u16 = 80;

    /// The rows it holds.
    pub const ROWS: u16 = 24;

    /// Runs the `yunta` binary at `bin` in `dir`, with `YUNTA_HOME`
    /// pointed at `home` and a terminal on all three of its streams.
    ///
    /// Use the [`yunta_on_terminal!`](crate::yunta_on_terminal) macro
    /// rather than calling this directly — it fills in the binary path
    /// from the calling crate's `CARGO_BIN_EXE_yunta`.
    pub fn open(bin: &Path, dir: &Path, home: &Path, args: &[&str]) -> Self {
        Self::open_with(bin, dir, home, args, &[])
    }

    /// [`Terminal::open`], with `vars` set on top of the hermetic
    /// environment — for a test about what a variable the reader set
    /// changes.
    pub fn open_with(
        bin: &Path,
        dir: &Path,
        home: &Path,
        args: &[&str],
        vars: &[(&str, &str)],
    ) -> Self {
        Self::open_sized(bin, dir, home, args, vars, (Self::COLUMNS, Self::ROWS))
    }

    /// [`Terminal::open_with`], on a terminal of `(columns, rows)` cells, for a
    /// surface that lays out by the room it is given.
    pub fn open_sized(
        bin: &Path,
        dir: &Path,
        home: &Path,
        args: &[&str],
        vars: &[(&str, &str)],
        (columns, rows): (u16, u16),
    ) -> Self {
        let screen = Winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = openpty(Some(&screen), None).expect("a pty for the run to be watched on");
        let slave = |what: &str| -> Stdio {
            pty.slave
                .try_clone()
                .unwrap_or_else(|e| panic!("the pty's {what} end: {e}"))
                .into()
        };
        let mut command = Command::new(bin);
        crate::bin::hermetic(&mut command, dir, home);
        command.envs(vars.iter().copied());
        let child = command
            .args(args)
            .stdin(slave("stdin"))
            .stdout(slave("stdout"))
            .stderr(slave("stderr"))
            .spawn()
            .expect("failed to run the yunta binary");
        let keyboard = std::fs::File::from(
            pty.master
                .try_clone()
                .expect("a second handle on the pty's master end"),
        );
        let drawn = Arc::new(Mutex::new(String::new()));
        Self {
            child,
            keyboard,
            reader: Some(collect(std::fs::File::from(pty.master), drawn.clone())),
            drawn,
            ended: None,
            columns,
            rows,
        }
    }

    /// Everything the run has drawn so far, as a person reads it: the
    /// sequences that move the cursor and clear rows are kept, because
    /// they are how a drawing is read here, and paint is not, because a
    /// color changes how a word looks and never which word it is.
    pub fn drawn(&self) -> String {
        unpainted(
            &self
                .drawn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// What the run drew on the other screen, one entry each time it held it: from its
    /// holding to its giving back, or to now. Read where the screen a person sees cannot
    /// tell a fresh drawing from what the other screen kept of the last one.
    pub fn drawn_on_the_other_screen(&self) -> Vec<String> {
        let drawn = self.drawn();
        drawn
            .split(ALT_SCREEN_ON)
            .skip(1)
            .map(|held| {
                held.split(ALT_SCREEN_OFF)
                    .next()
                    .unwrap_or(held)
                    .to_string()
            })
            .collect()
    }

    /// What a person sees on this terminal now: every row as the screen holds
    /// it, whatever was drawn over what on the way there. A drawing still on its
    /// way is out of sight, as a terminal that takes one whole keeps it, so what a
    /// test reads after waiting for one row of a drawing holds all of its rows.
    pub fn screen(&self) -> Screen {
        let drawn = self
            .drawn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Screen::of(&drawn, (self.columns, self.rows))
    }

    /// Waits until `needle` stands on the screen, failing with `what` and the
    /// screen as it is.
    pub fn wait_on_screen(&self, needle: &str, what: &str) {
        wait_until(
            || self.screen().holds(needle),
            || format!("{what}\non the screen:\n{}", self.screen().rows.join("\n")),
        );
    }

    /// Waits until `needle` has been drawn, failing with `what` and
    /// everything drawn so far.
    ///
    /// It returns once `needle` is among what was drawn, which says a
    /// redraw began, never that it ended: a test that then counts the rows
    /// a redraw wrote or erased can read it half written under load. Wait
    /// for what the redraw writes last instead.
    pub fn wait_for(&self, needle: &str, what: &str) {
        wait_until(
            || self.drawn().contains(needle),
            || format!("{what}\ndrawn so far:\n{}", self.drawn()),
        );
    }

    /// Puts this terminal in raw mode — the state a prompt's own read
    /// puts it in for as long as it is reading — and leaves it there for
    /// the whole run.
    ///
    /// Two things that state settles. Its signal keys are off, so
    /// Ctrl-C is a byte the program reads rather than a signal the
    /// kernel sends, and a prompt that read that byte as text would
    /// swallow a person's attempt to stop the run. Its echo is off, so
    /// what the test reads back is what the run drew: a terminal echoes
    /// typed keys itself between the reads a prompt makes, and a
    /// measurement of the rows a surface drew would otherwise be
    /// counting the line discipline's.
    ///
    /// A test cannot get at the other half of the signal story.
    /// Delivering a signal from the line discipline needs the run to be
    /// this terminal's foreground process group, which needs `setsid` in
    /// the child between fork and exec, and this workspace forbids the
    /// `unsafe` that reaching for it takes. [`Terminal::interrupt`] is
    /// the signal a test can send.
    pub fn raw(&self) {
        let mut termios = tcgetattr(&self.keyboard).expect("the pty's current mode");
        cfmakeraw(&mut termios);
        tcsetattr(&self.keyboard, SetArg::TCSANOW, &termios).expect("the pty takes raw mode");
    }

    /// The card on this terminal pointed down past each of `below` to `option`, which is
    /// chosen.
    pub fn choose(&mut self, below: &[&str], option: &str) {
        for next in below.iter().chain([&option]) {
            if !self.screen().holds(&format!("▸ {next}")) {
                self.keys("\u{1b}[B");
            }
            self.wait_on_screen(&format!("▸ {next}"), "the card never pointed there");
        }
        self.keys("\r");
    }

    /// Types `keys`, exactly as a keyboard delivers them.
    pub fn keys(&mut self, keys: &str) {
        self.keyboard
            .write_all(keys.as_bytes())
            .and_then(|()| self.keyboard.flush())
            .expect("the run is still reading its keyboard");
    }

    /// Sends the run an interrupt from outside — what `kill -INT` does,
    /// and what a shell's Ctrl-C does to a run in its foreground group.
    pub fn interrupt(&self) {
        self.signal(Signal::SIGINT);
    }

    /// Asks the run to end — what `kill` does by default.
    pub fn terminate(&self) {
        self.signal(Signal::SIGTERM);
    }

    /// Tells the run its terminal hung up — what closing the window it runs in does.
    pub fn hang_up(&self) {
        self.signal(Signal::SIGHUP);
    }

    /// Gives this terminal `columns` by `rows`, and tells the run, as a terminal whose
    /// window is resized tells the program in front: the run is no session's leader here,
    /// so the harness sends the signal the line discipline would.
    pub fn resized(&mut self, columns: u16, rows: u16) {
        let size = rustix::termios::Winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        rustix::termios::tcsetwinsize(&self.keyboard, size).expect("the pty takes a size");
        self.columns = columns;
        self.rows = rows;
        self.signal(Signal::SIGWINCH);
    }

    /// The run's process.
    /// The id of the process at this terminal.
    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    fn pid(&self) -> Pid {
        Pid::from_raw(self.process_id() as i32)
    }

    /// `signal` sent to the run.
    fn signal(&self, signal: Signal) {
        kill(self.pid(), signal).expect("the run is alive to be signalled");
    }

    /// Whether this terminal still reads a line at a time and echoes
    /// what is typed into it — the line discipline a shell hands a
    /// program, and the one a program owes back.
    ///
    /// A prompt's key read turns both off for as long as it reads. A
    /// run that leaves without putting them back hands the shell it
    /// returns to a terminal that shows nothing a person types into it
    /// and answers no Enter, and nothing that runs next puts either
    /// back.
    pub fn line_discipline_is_back(&self) -> bool {
        tcgetattr(&self.keyboard)
            .expect("the pty says what mode it is in")
            .local_flags
            .contains(LocalFlags::ECHO | LocalFlags::ICANON)
    }

    /// Whether the run put the terminal's cursor back as often as it
    /// took it away.
    ///
    /// A list hides the cursor while it draws. A prompt that ends and
    /// leaves it hidden hands the shell the run returns to a terminal
    /// with no cursor in it, which nothing that runs next puts back.
    pub fn cursor_is_back(&self) -> bool {
        let drawn = self.drawn();
        drawn.matches(HIDE_CURSOR).count() == drawn.matches(SHOW_CURSOR).count()
    }

    /// Whether the run cleared the last row carrying `pinned` off the
    /// terminal before it drew `next`.
    ///
    /// This is how a region giving the terminal up to a prompt reads on
    /// the wire: the rows it had pinned are cleared, and only then does
    /// the prompt draw its first one. A prompt drawn under rows still
    /// pinned is a prompt the next redraw of those rows lands on top of.
    pub fn cleared_before(&self, pinned: &str, next: &str) -> bool {
        let drawn = self.drawn();
        let Some(next_at) = drawn.find(next) else {
            return false;
        };
        let Some(before) = drawn.get(..next_at) else {
            return false;
        };
        let Some(pinned_at) = before.rfind(pinned) else {
            return false;
        };
        before
            .get(pinned_at..)
            .is_some_and(|between| between.contains(CLEAR_ROW))
    }

    /// Whether the run came back saying it did everything it was asked
    /// to do — a run that parked reports a verdict instead.
    pub fn ran_to_the_end(&self) -> bool {
        self.ended.is_some_and(|status| status.success())
    }

    /// The code the run's process exited with, once it has ended with
    /// one.
    pub fn exit_code(&self) -> Option<i32> {
        self.ended.and_then(|status| status.code())
    }

    /// Waits for the run's process to end and returns everything it
    /// drew. Fails, rather than hanging, on a run that never ends.
    pub fn ended(&mut self) -> String {
        let Self { child, drawn, .. } = self;
        let ended = wait_for(
            || child.try_wait().ok().flatten(),
            || {
                let drawn = drawn.lock().map(|d| d.clone()).unwrap_or_default();
                format!("the run never ended\ndrawn so far:\n{drawn}")
            },
        );
        self.ended = Some(ended);
        if let Some(reader) = self.reader.take() {
            drop(reader.join());
        }
        self.drawn()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // A run still going is stopped the way a person stops one, so it
        // closes the process trees it started. Killed outright, it
        // leaves its nodes running in their own groups, and the reader
        // below waits on them for as long as they run.
        if self.ended.is_none() && matches!(self.child.try_wait(), Ok(None)) {
            if let Err(error) = kill(self.pid(), Signal::SIGINT) {
                eprintln!("could not interrupt the run while dropping its terminal: {error}");
            }
            let deadline = Instant::now() + WAIT_DEADLINE;
            while Instant::now() < deadline && matches!(self.child.try_wait(), Ok(None)) {
                std::thread::yield_now();
            }
        }
        drop(self.child.kill());
        drop(self.child.wait());
        if let Some(reader) = self.reader.take() {
            drop(reader.join());
        }
    }
}

/// Reads the master end for as long as the run holds the slave, keeping
/// everything it draws.
///
/// A thread of its own because the read is blocking: a test that only
/// read when it wanted to assert would let the pty's buffer fill and
/// stall the run it is watching.
fn collect(mut screen: std::fs::File, into: Arc<Mutex<String>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        while let Ok(read) = screen.read(&mut buffer) {
            if read == 0 {
                break;
            }
            if let Ok(mut drawn) = into.lock() {
                drawn.push_str(&String::from_utf8_lossy(&buffer[..read]));
            }
        }
    })
}

/// Where a run under [`Terminal`] keeps its runs, so a test can read
/// what one produced: `YUNTA_HOME/runs/<id>/`.
pub fn runs_root(home: &Path) -> PathBuf {
    home.join("runs")
}

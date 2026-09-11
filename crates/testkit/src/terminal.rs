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

use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, LocalFlags, SetArg};
use nix::unistd::Pid;

use crate::wait::{wait_for, wait_until};

/// What a terminal is told to do, as the bytes a run writes to say it:
/// the introducer every sequence opens with, and the three sequences
/// this harness reads a run's drawing by.
const ESCAPE: char = '\u{1b}';
const HIDE_CURSOR: &str = "\u{1b}[?25l";
const SHOW_CURSOR: &str = "\u{1b}[?25h";
const CLEAR_ROW: &str = "\u{1b}[2K";

/// What the run is told its terminal is: one that draws text and colour
/// like any other, so what a test measures is what the run's own rules
/// chose rather than what the machine running the suite declares.
const TERM: &str = "xterm-256color";

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
        let screen = Winsize {
            ws_row: Self::ROWS,
            ws_col: Self::COLUMNS,
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
        let child = Command::new(bin)
            .args(args)
            .current_dir(dir)
            .env("YUNTA_HOME", home)
            // The run reads both of these to decide what it may draw, so
            // they are stated here rather than inherited: a suite run
            // under `NO_COLOR`, or under a terminal calling itself dumb,
            // would otherwise measure the append-only downgrade where the
            // test asked for a terminal.
            .env("TERM", TERM)
            .env_remove("NO_COLOR")
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
        }
    }

    /// Everything the run has drawn so far, escape sequences and all.
    pub fn drawn(&self) -> String {
        self.drawn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Waits until `needle` has been drawn, failing with `what` and
    /// everything drawn so far.
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
        kill(Pid::from_raw(self.child.id() as i32), Signal::SIGINT)
            .expect("the run is alive to be interrupted");
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

    /// How many rows a redrawing list wrote after `from`, and how many
    /// it then went back over to clear them.
    ///
    /// Everything from the row `from` names to the first escape
    /// sequence after it, and the `ESC [ n A` that sequence opens with.
    /// A list that clears more rows than it wrote takes the rows above
    /// it with them — the evidence a person is deciding on.
    pub fn drew_and_cleared(&self, from: &str) -> (usize, usize) {
        let drawn = self.drawn();
        let list = drawn
            .split_once(from)
            .map(|(_, rest)| rest)
            .unwrap_or_else(|| panic!("the list never drew `{from}`:\n{drawn}"));
        let (rows, cleared) = list
            .split_once(ESCAPE)
            .unwrap_or_else(|| panic!("the list never went back over what it drew:\n{drawn}"));
        let cleared = cleared
            .strip_prefix('[')
            .and_then(|rest| rest.split_once('A'))
            .and_then(|(rows, _)| rows.parse().ok())
            .unwrap_or_else(|| {
                panic!("what the list did after its rows was not a move back over them:\n{drawn}")
            });
        // `from` is the first row's own text, so that row is counted
        // here rather than read off `lines()`.
        (
            rows.lines().count() + usize::from(!rows.starts_with('\n')) - 1,
            cleared,
        )
    }

    /// Every row a line being typed was redrawn on: what followed each
    /// clearing of that row, up to whatever was written next.
    pub fn rows_redrawn(&self) -> Vec<String> {
        self.drawn()
            .split(CLEAR_ROW)
            .skip(1)
            .map(|rest| {
                rest.split([ESCAPE, '\n', '\r'])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    /// Whether the run came back saying it did everything it was asked
    /// to do — a run that parked reports a verdict instead.
    pub fn ran_to_the_end(&self) -> bool {
        self.ended.is_some_and(|status| status.success())
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

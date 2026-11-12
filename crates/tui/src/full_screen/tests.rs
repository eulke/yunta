//! The terminal held and given back, on a real terminal: the test runs its own binary
//! again in a pseudo-terminal, where it holds the terminal and leaves, panics, or stops
//! and goes on, and reads what the terminal is left as.

use std::io::Write;

use surco::paint::{ALT_SCREEN_OFF, ALT_SCREEN_ON, ALT_SCROLL_OFF, ALT_SCROLL_ON};
use yunta_testkit::Terminal;

use super::FullScreen;

/// What the binary run again writes once the terminal is held.
const HELD: &str = "the terminal is held";

/// What it writes once it holds the terminal again after a stop.
const HELD_AGAIN: &str = "the terminal is held again";

/// What it writes once a panic that unwound through the holding is caught.
const WENT_ON: &str = "went on after the panic";

/// The binary run again in a pseudo-terminal, running the one test `holding`, which holds
/// the terminal.
fn holding(holding: &str) -> Terminal {
    let binary = std::env::current_exe().expect("the test's own binary");
    let root = tempfile::tempdir().expect("a directory for the run");
    let test = format!("full_screen::tests::{holding}");
    let args = ["--exact", test.as_str(), "--nocapture"];
    Terminal::open_sized(
        &binary,
        root.path(),
        &root.path().join("home"),
        &args,
        &[],
        (Terminal::COLUMNS, Terminal::ROWS),
    )
}

/// Whether this process is the binary run again for the one test `holding`: its arguments
/// name that test exactly. Run with the rest, a test that holds the terminal holds nothing.
fn run_again_for(holding: &str) -> bool {
    let arguments: Vec<String> = std::env::args().collect();
    arguments.iter().any(|argument| argument == "--exact")
        && arguments
            .iter()
            .any(|argument| argument == &format!("full_screen::tests::{holding}"))
}

/// What `terminal` is left as is what it was before the binary held it.
fn given_back(terminal: &mut Terminal) {
    let drawn = terminal.ended();
    let screen = terminal.screen();
    assert!(!screen.alternate, "the main screen is back\n{drawn}");
    assert!(!screen.cursor_hidden, "the cursor is shown\n{drawn}");
    assert!(
        !screen.bracketed_paste,
        "pastes are no longer marked\n{drawn}"
    );
    assert!(!screen.mouse_captured, "the mouse was never taken\n{drawn}");
    assert!(
        terminal.line_discipline_is_back(),
        "raw mode is left\n{drawn}"
    );
    let last_on = drawn
        .rfind(ALT_SCROLL_ON)
        .expect("the wheel was sent arrows");
    let last_off = drawn.rfind(ALT_SCROLL_OFF).expect("the wheel was put back");
    assert!(last_off > last_on, "the wheel is put back last\n{drawn}");
}

#[test]
fn leaving_gives_the_terminal_back() {
    let mut terminal = holding("held_then_left");
    given_back(&mut terminal);
    let drawn = terminal.drawn();
    let entered = drawn.find(ALT_SCREEN_ON).expect("the other screen taken");
    let held = drawn.find(HELD).expect("the terminal held");
    assert!(entered < held, "held on the other screen\n{drawn}");
}

#[test]
fn a_panic_gives_the_terminal_back_with_no_drop_run() {
    let mut terminal = holding("held_then_panicked");
    given_back(&mut terminal);
    assert_eq!(
        terminal.exit_code(),
        None,
        "the run aborted, as a release build does"
    );
}

/// A stop gives the terminal back before the process stands, and holds it again once it
/// goes on: what a test reads is the terminal given back between the two holdings. Under a
/// shell's job control the process stops for real, and the test sends what `fg` sends
/// while it waits; in a group no shell controls it never stops.
#[test]
fn a_stop_gives_the_terminal_back_before_it_and_holds_it_again_after() {
    let mut terminal = holding("held_then_stopped");
    yunta_testkit::wait_until(
        || {
            terminal.go_on();
            terminal.drawn().contains(HELD_AGAIN)
        },
        || format!("never held again\n{}", terminal.drawn()),
    );
    given_back(&mut terminal);
    let drawn = terminal.drawn();
    let first = drawn.find(HELD).expect("held first");
    let again = drawn.find(HELD_AGAIN).expect("held again");
    let between = &drawn[first..again];
    assert!(
        between.contains(ALT_SCROLL_OFF),
        "the wheel put back before the stop"
    );
    assert!(
        between.contains(ALT_SCREEN_OFF),
        "the main screen back before the stop"
    );
    assert!(
        between.rfind(ALT_SCROLL_ON) > between.rfind(ALT_SCREEN_OFF),
        "held again after it"
    );
}

/// A panic that unwinds through the holding gives the terminal back, and the process goes
/// on where the panic is caught: the holding dropped while a panic unwinds puts no panic
/// hook back, which a panicking thread may not do.
#[test]
fn a_panic_unwinding_through_the_holding_gives_the_terminal_back_and_goes_on() {
    let mut terminal = holding("held_then_unwound");
    given_back(&mut terminal);
    let drawn = terminal.drawn();
    assert!(drawn.contains(WENT_ON), "the process went on\n{drawn}");
    assert_eq!(terminal.exit_code(), Some(0), "{drawn}");
}

/// What `a_panic_unwinding_through_the_holding_gives_the_terminal_back_and_goes_on` runs
/// in a pseudo-terminal: the terminal held, a panic unwound through the holding and
/// caught, then the process going on.
#[test]
fn held_then_unwound() {
    if !run_again_for("held_then_unwound") {
        return;
    }
    let caught = std::panic::catch_unwind(|| {
        let _held = FullScreen::held().expect("a terminal to hold");
        said(HELD);
        panic!("made to, while the terminal is held");
    });
    assert!(caught.is_err(), "the panic unwound to here");
    said(WENT_ON);
}

/// What `leaving_gives_the_terminal_back` runs in a pseudo-terminal: the terminal held,
/// then left.
#[test]
fn held_then_left() {
    if !run_again_for("held_then_left") {
        return;
    }
    let held = FullScreen::held().expect("a terminal to hold");
    said(HELD);
    drop(held);
}

/// What `a_panic_gives_the_terminal_back_with_no_drop_run` runs in a pseudo-terminal: the
/// terminal held, then a panic under a hook that aborts, so no drop runs.
#[test]
fn held_then_panicked() {
    if !run_again_for("held_then_panicked") {
        return;
    }
    std::panic::set_hook(Box::new(|_| std::process::abort()));
    let _held = FullScreen::held().expect("a terminal to hold");
    said(HELD);
    panic!("made to, while the terminal is held");
}

/// What `a_stop_gives_the_terminal_back_before_it_and_holds_it_again_after` runs in a
/// pseudo-terminal: the terminal held, then stopped, then held again.
#[test]
fn held_then_stopped() {
    if !run_again_for("held_then_stopped") {
        return;
    }
    let mut held = FullScreen::held().expect("a terminal to hold");
    said(HELD);
    held.suspend().expect("held again after the stop");
    said(HELD_AGAIN);
    drop(held);
}

/// `words` written on the terminal held, at its top.
fn said(words: &str) {
    let mut out = std::io::stdout();
    write!(out, "\r{words}").expect("the terminal takes a write");
    out.flush().expect("the terminal takes a flush");
}

//! Bare `yunta`: where it opens the application, how it leaves and gives the terminal
//! back, how it meets a resize, a suspend and a signal, and where it prints as it always
//! has instead.

use std::path::Path;

use surco::paint::{ALT_SCREEN_OFF, ALT_SCREEN_ON};
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{
    stdout, wait_until, yunta_in, yunta_on_terminal, yunta_on_terminal_sized, Checkout,
};

use super::{given_back, opened, THE_FIELD};

/// A word of the field, which a drawing writes whole: it moves the cursor over the spaces
/// between words rather than write them.
const FIELD_WORD: &str = "repository";

/// A repository set up for Yunta under `root`, with one run that finished.
fn with_a_run(root: &Path) -> Checkout {
    let checkout = Checkout::under(root)
        .config("defaults:\n  isolation: none\n")
        .workflow("done", FINISHES)
        .committed();
    let ran = yunta_in!(&checkout.repo, &checkout.home, &["run", "done.yaml"]);
    assert!(ran.status.success(), "{}", yunta_testkit::stderr(&ran));
    checkout
}

#[test]
fn bare_yunta_opens_on_the_field_and_q_leaves_with_the_terminal_as_it_was() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

#[test]
fn ctrl_c_leaves_with_the_code_an_interrupt_exits_with() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.keys("\u{3}");
    assert_eq!(given_back(&mut terminal), Some(130));
}

#[test]
fn a_terminate_ends_it_with_the_terminal_given_back() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.terminate();
    assert_eq!(given_back(&mut terminal), Some(143));
}

/// An interrupt from outside, as `kill -INT` sends, ends it as Ctrl-C does.
#[test]
fn an_interrupt_from_outside_ends_it_with_the_code_an_interrupt_exits_with() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.interrupt();
    assert_eq!(given_back(&mut terminal), Some(130));
}

/// The terminal hung up, as closing its window does: it ends with the terminal given back.
#[test]
fn a_hang_up_ends_it_with_the_terminal_given_back() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.hang_up();
    assert_eq!(given_back(&mut terminal), Some(129));
}

#[test]
fn a_resize_lays_it_out_again_and_too_little_room_says_what_it_needs() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.resized(100, 30);
    wait_until(
        || {
            let screen = terminal.screen();
            screen.rows.len() == 30 && screen.holds(THE_FIELD) && !screen.rows[29].is_empty()
        },
        || format!("never laid out at 100 by 30\n{:#?}", terminal.screen().rows),
    );
    terminal.resized(59, 14);
    terminal.wait_on_screen("make it larger", "too little room never said so");
    terminal.resized(80, 24);
    terminal.wait_on_screen(THE_FIELD, "the field never came back with the room");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// Ctrl-Z gives the terminal back before the process stops and holds it again after,
/// drawing the field whole on the other screen, where the application goes on until it is
/// left. The test sends what `fg` sends while it waits, which a shell's job control needs
/// and a group no shell controls does not.
#[test]
fn ctrl_z_gives_the_terminal_back_and_holds_it_again() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = opened(&with_a_run(root.path()));
    terminal.keys("\u{1a}");
    wait_until(
        || {
            terminal.go_on();
            let held = terminal.drawn_on_the_other_screen();
            held.len() == 2 && held[1].contains(FIELD_WORD) && terminal.screen().alternate
        },
        || {
            format!(
                "the field was never drawn again on the other screen\n{}",
                terminal.drawn()
            )
        },
    );
    let drawn = terminal.drawn();
    let second = drawn.rfind(ALT_SCREEN_ON).expect("held again");
    assert!(
        drawn[..second].contains(ALT_SCREEN_OFF),
        "given back between\n{drawn}"
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

#[test]
fn outside_a_repository_it_opens_on_the_signpost() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let mut terminal = yunta_on_terminal_sized!(&outside, &root.path().join("home"), &[], (80, 24));
    terminal.wait_on_screen("not in a repository", "the signpost never stood");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// A terminal that says it draws nothing beyond text, and a pipe, get the field printed,
/// as bare `yunta` always printed it.
#[test]
fn a_dumb_terminal_and_a_pipe_get_the_field_printed() {
    let root = tempfile::tempdir().unwrap();
    let checkout = with_a_run(root.path());
    let mut dumb = yunta_on_terminal!(&checkout.repo, &checkout.home, &[], &[("TERM", "dumb")]);
    let drawn = dumb.ended();
    assert_eq!(dumb.exit_code(), Some(0), "{drawn}");
    assert!(drawn.contains(THE_FIELD), "{drawn}");
    assert!(
        !drawn.contains(ALT_SCREEN_ON),
        "no other screen taken\n{drawn}"
    );
    let piped = yunta_in!(&checkout.repo, &checkout.home, &[]);
    assert!(piped.status.success());
    assert!(stdout(&piped).contains(THE_FIELD), "{}", stdout(&piped));
}

//! Watching a run in the application: a run started detached, opened from the field and
//! followed to its end, and onto the run a promotion hands the work to; leaving while a
//! run moves leaves it moving. The application only follows: the run's engine is the
//! process the run was started in, never the application's.

use std::path::Path;

use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{handle, run_ids, wait_until, yunta_on_terminal, Checkout, HeldRun};

use super::{engine_of, following, given_back, outcome, FINISHED};

/// The binary these tests run.
fn yunta() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_yunta"))
}

#[test]
fn a_run_started_detached_is_followed_from_the_field_to_its_end() {
    let held = HeldRun::started_detached(yunta());
    let mut terminal = following(&held.checkout);
    assert_ne!(
        engine_of(&held.checkout, &held.id),
        u64::from(terminal.process_id()),
        "the application follows the run and never runs it"
    );
    held.hold.let_go();
    wait_until(
        || terminal.screen().holds(FINISHED),
        || {
            format!(
                "the run was never followed to its end\n{:#?}",
                terminal.screen().rows
            )
        },
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
    assert_eq!(outcome(&held.checkout, &held.id), "finished");
}

/// A run promoted at its own terminal while the application follows it is followed onto
/// the run that succeeds it, whose page is read in its place.
#[test]
fn a_promotion_is_followed_onto_its_successor() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", PROMOTABLE)
        .committed();
    let mut attached = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--mode", "quick"]
    );
    attached.wait_on_screen("▸ retry", "the exhausted reroute never asked");
    let first = run_ids(&checkout.home).remove(0);
    let mut terminal = following(&checkout);

    attached.choose(&["abort"], "promote");
    attached.wait_on_screen("▸ retry", "the successor never asked");
    let successor = run_ids(&checkout.home)
        .into_iter()
        .find(|run| *run != first)
        .expect("a successor");
    terminal.wait_on_screen(
        "promotable (full)",
        "the application never followed the promotion",
    );
    assert!(
        terminal.screen().holds(handle(&successor)),
        "the successor's handle on the trail\n{:#?}",
        terminal.screen().rows
    );
    attached.choose(&[], "abort");
    attached.ended();
    assert_eq!(attached.exit_code(), Some(1), "abort fails the successor");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// Leaving the application while the run it follows moves leaves the run moving, as
/// `yunta status` reads it, until it is let go and finishes.
#[test]
fn leaving_while_a_run_moves_leaves_it_moving() {
    let held = HeldRun::started_detached(yunta());
    let mut terminal = following(&held.checkout);
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
    assert_eq!(outcome(&held.checkout, &held.id), "running");
    held.hold.let_go();
    wait_until(
        || outcome(&held.checkout, &held.id) == "finished",
        || {
            format!(
                "the run never finished: {}",
                outcome(&held.checkout, &held.id)
            )
        },
    );
}

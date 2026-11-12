//! Watching a run in the application: a run started detached, opened from the field and
//! followed to its end, and onto the run a promotion hands the work to; leaving while a
//! run moves leaves it moving. The application only follows: the run's engine is the
//! process the run was started in, never the application's.

use std::path::Path;

use serde_json::Value;
use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{
    handle, run_ids, runs_root, stdout, wait_until, yunta_at, yunta_on_terminal, Checkout, HeldRun,
    Terminal,
};

use super::{given_back, opened};

/// What the run's legend says once its nodes stand beside its page, the pointer on one:
/// what only the run's following brings, since a run not yet read has no nodes to point at.
const FOLLOWED: &str = "move along its nodes";

/// What the run's page says at its head once the run finished: in sight however long its
/// chronicle grows below.
const FINISHED: &str = "(default) · finished";

/// The binary these tests run.
fn yunta() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_yunta"))
}

/// Bare `yunta` in `checkout`'s repository, its first run opened from the field and
/// followed.
fn following(checkout: &Checkout) -> Terminal {
    let mut terminal = opened(checkout);
    terminal.keys("\r");
    terminal.wait_on_screen(FOLLOWED, "the run opened from the field was never followed");
    terminal
}

/// The process the engine of `held` runs in, as the run's registry names it while the
/// engine lives.
fn engine_of(held: &HeldRun) -> u64 {
    let registry = runs_root(&held.checkout.home)
        .join(&held.id)
        .join("scratch")
        .join("engine.json");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(registry).unwrap()).unwrap();
    file["engine_pid"].as_u64().expect("the engine's process")
}

/// How `yunta status` says the run `id` stands, as its document names it.
fn outcome(checkout: &Checkout, id: &str) -> String {
    let status = yunta_at!(checkout, &["status", id, "--json"]);
    let document: Value = serde_json::from_str(&stdout(&status)).expect("one document");
    document["outcome"].as_str().unwrap_or_default().to_string()
}

#[test]
fn a_run_started_detached_is_followed_from_the_field_to_its_end() {
    let held = HeldRun::started_detached(yunta());
    let mut terminal = following(&held.checkout);
    assert_ne!(
        engine_of(&held),
        u64::from(terminal.process_id()),
        "the application follows the run and never runs it"
    );
    held.let_go();
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
    held.let_go();
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

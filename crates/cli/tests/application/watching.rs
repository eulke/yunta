//! Watching a run in the application: a run started detached, opened from the field and
//! followed to its end, and onto the run a promotion hands the work to; leaving while a
//! run moves leaves it moving. With no key pressed, the run's screen goes on as shells
//! move the run: a gate answered, a stop and a resume. A node picked, and the run's stats
//! opened and closed, never stop it from moving. Each move is read once the one before it
//! stands, the run held by a word between the two. The application only follows: the
//! run's engine is another process, never the application.

use std::path::Path;
use std::process::Stdio;

use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{
    handle, run_ids, stderr, wait_until, yunta_at, yunta_on_terminal, Checkout, HeldRun, Hold,
    Terminal,
};

use super::{engine_of, following, given_back, outcome, parked, FINISHED};

/// The word the node after the gate waits for before the run finishes.
const FINISH: &str = "finish";

/// The marker the first of two held nodes writes once it started.
const FIRST_STARTED: &str = "first-started";

/// The word the first of two held nodes waits for before the second starts.
const FIRST: &str = "first";

/// The marker the second of two held nodes writes once it started.
const SECOND_STARTED: &str = "second-started";

/// The word the second of two held nodes waits for before the run finishes.
const SECOND: &str = "second";

/// The binary these tests run.
fn yunta() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_yunta"))
}

/// The workflow `gated`: a gate asks the person whether to go on; the node after it
/// holds until `hold` gives the word [`FINISH`], or until its marks are gone.
fn gated_then_held(hold: &Hold) -> String {
    format!(
        "name: gated\nnodes:\n  - {{ id: approve, kind: gate, assignee: lead, message: \"Go on?\", options: [go] }}\n  - id: after\n    kind: bash\n    run: \"until [ -f {finish} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n    depends_on: [approve]\n",
        finish = hold.word(FINISH).display(),
        marks = hold.marks().display(),
    )
}

/// The workflow `twice`: the node `first` says it started, then holds until `hold` gives
/// the word [`FIRST`]; the node `second` says it started, then holds until the word
/// [`SECOND`]. Each ends anyway once the marks are gone.
fn held_twice(hold: &Hold) -> String {
    format!(
        "name: twice\nnodes:\n  - id: first\n    kind: bash\n    run: \"echo in > {first_started}; until [ -f {first} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n  - id: second\n    kind: bash\n    run: \"echo in > {second_started}; until [ -f {second} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n    depends_on: [first]\n",
        first_started = hold.word(FIRST_STARTED).display(),
        first = hold.word(FIRST).display(),
        second_started = hold.word(SECOND_STARTED).display(),
        second = hold.word(SECOND).display(),
        marks = hold.marks().display(),
    )
}

/// Waits until every one of `shown` stands on `terminal`'s screen and none of `gone`
/// does, failing with `what` and the screen.
fn wait_until_shown(terminal: &Terminal, shown: &[&str], gone: &[&str], what: &str) {
    wait_until(
        || {
            let screen = terminal.screen();
            shown.iter().all(|row| screen.holds(row)) && !gone.iter().any(|row| screen.holds(row))
        },
        || {
            format!(
                "{what}\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
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

/// A run parked on its gate, answered from a shell, goes on on its screen with no key
/// pressed: its headline leaves needs you and the node after the gate works, then it is
/// read to its end.
#[test]
fn a_gate_answered_from_a_shell_goes_on_on_the_runs_screen() {
    let hold = Hold::new();
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", &gated_then_held(&hold))
        .committed();
    let id = parked(&checkout, &["run", "wf.yaml"]);
    let mut terminal = following(&checkout);
    let asked = format!("needs you: yunta resolve-gate {}", handle(&id));
    wait_until_shown(
        &terminal,
        &["(default) · needs you", &asked],
        &[],
        "the run parked never stood as needing you",
    );

    let answered = yunta_at!(&checkout, &["resolve-gate", &id, "go"]);
    assert!(answered.status.success(), "{}", stderr(&answered));
    wait_until_shown(
        &terminal,
        &["(default) · running", "after running for"],
        &["(default) · needs you", &asked],
        "the run answered never went on to its next node",
    );

    hold.give(FINISH);
    terminal.wait_on_screen(FINISHED, "the run was never read to its end");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// A run stopped from a shell stands stopped on its screen, and once a shell resumes it,
/// it is read on to its end, with no key pressed.
#[test]
fn a_run_stopped_then_resumed_from_a_shell_is_read_on_to_its_end() {
    let held = HeldRun::started_detached(yunta());
    let mut terminal = following(&held.checkout);
    terminal.wait_on_screen("hold running for", "the held node never worked");

    let cancelled = yunta_at!(&held.checkout, &["cancel", &held.id]);
    assert!(cancelled.status.success(), "{}", stderr(&cancelled));
    terminal.wait_on_screen("(default) · stopped", "the run stopped never stood stopped");

    let mut resumed = held
        .checkout
        .command(yunta())
        .args(["resume", &held.id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the run resumes");
    wait_until_shown(
        &terminal,
        &["(default) · running", "hold running for"],
        &["(default) · stopped"],
        "the run resumed was never read going on",
    );

    held.hold.let_go();
    terminal.wait_on_screen(FINISHED, "the run resumed was never read to its end");
    assert!(resumed.wait().expect("the resume ends").success());
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// A node picked, and the run's stats opened and closed while the run moves, the rows
/// stand where the run went meanwhile, and go on moving.
#[test]
fn a_node_picked_and_the_stats_closed_stand_where_the_run_went_meanwhile() {
    let hold = Hold::new();
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", &held_twice(&hold))
        .committed();
    let detached = yunta_at!(&checkout, &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    wait_until(
        || hold.word(FIRST_STARTED).exists(),
        || "the run never started".to_string(),
    );
    let mut terminal = following(&checkout);
    terminal.wait_on_screen("first running for", "the first node never worked");
    terminal.keys("\r");
    terminal.wait_on_screen("node first of run", "the node's page never stood");
    terminal.keys("s");
    terminal.wait_on_screen("› stats", "its stats never opened");

    hold.give(FIRST);
    wait_until(
        || hold.word(SECOND_STARTED).exists(),
        || "the run never went on to its second node".to_string(),
    );
    terminal.keys("\u{1b}");
    wait_until_shown(
        &terminal,
        &[
            "node first of run",
            "second running for",
            "1 of 2 nodes done",
        ],
        &["› stats"],
        "back on the run, the rows never stood where it went",
    );

    hold.give(SECOND);
    terminal.wait_on_screen("2 of 2 nodes done", "the rows never went on moving");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

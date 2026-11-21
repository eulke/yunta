//! The field moving with no key pressed while other processes move its runs: a run started
//! detached comes under what runs and parks on its gate under needs you, which the tally
//! and the attention row say; answered from a shell, it leaves needs you, and it goes among
//! the closed once it finishes; a run a shell cancels moves under what is stopped; a run
//! whose engine is killed moves under stalled, its log holding nothing past the kill. Each
//! move is read once the one before it stands, the run held by a word between the two, and
//! every run's engine is another process's, never the application's.

use std::path::Path;

use yunta_core::process::signal::{liveness, signal_process, Liveness, Signal};
use yunta_core::Pid;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{
    full_run_id, handle, heads, run_id_from, stderr, wait_until, yunta_at, Checkout, HeldRun, Hold,
    Screen, Terminal,
};

use super::{engine_of, given_back, opened, outcome};

/// The word the run's first node waits for before the run goes on to its gate.
const PARK: &str = "park";

/// The word the node after the gate waits for before the run finishes.
const FINISH: &str = "finish";

/// The marker the node after the gate writes once it started.
const RESUMED: &str = "resumed";

/// The heading of the runs that move.
const RUNNING: &str = "running";

/// The heading of the runs that wait on a decision.
const NEEDS_YOU: &str = "needs you, oldest first";

/// The headings the field groups its open runs under, each a row of its own.
const GROUPS: [&str; 5] = ["asking now", NEEDS_YOU, "stalled", "stopped", RUNNING];

/// The binary these tests run.
fn yunta() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_yunta"))
}

/// The workflow `moves`: its first node says it started, then holds until `hold` gives
/// the word [`PARK`]; a gate asks the person whether to go on; the node after it says it
/// started, then holds until the word [`FINISH`]. Each node ends anyway once the marks are
/// gone.
fn held_around_a_gate(hold: &Hold) -> String {
    format!(
        "name: moves\nnodes:\n  - id: held\n    kind: bash\n    run: \"echo in > {started}; until [ -f {park} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n  - {{ id: approve, kind: gate, assignee: lead, message: \"Go on?\", options: [go], depends_on: [held] }}\n  - id: after\n    kind: bash\n    run: \"echo in > {resumed}; until [ -f {finish} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n    depends_on: [approve]\n",
        started = hold.marks().join("started").display(),
        park = hold.word(PARK).display(),
        resumed = hold.word(RESUMED).display(),
        finish = hold.word(FINISH).display(),
        marks = hold.marks().display(),
    )
}

/// A repository whose workflow `wf` is [`held_around_a_gate`] by `hold`, and whose
/// workflow `done` finishes at once.
fn repository(hold: &Hold) -> Checkout {
    Checkout::new()
        .working_in_place()
        .workflow("done", FINISHES)
        .workflow("wf", &held_around_a_gate(hold))
        .committed()
}

/// The run of `wf` started detached in `checkout`, from a shell: its whole id.
fn started_detached(checkout: &Checkout) -> String {
    let detached = yunta_at!(checkout, &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    full_run_id(&checkout.home, &run_id_from(&detached))
}

/// The heading of the group the row of the run `id` stands under on `screen`: none where
/// no row names it.
fn group_of<'a>(screen: &'a Screen, id: &str) -> Option<&'a str> {
    let at = screen
        .rows
        .iter()
        .position(|row| row.contains(handle(id)))?;
    screen.rows[..at]
        .iter()
        .rev()
        .map(|row| row.trim())
        .find(|row| GROUPS.contains(row))
}

/// The row the screen counts its runs on, beside where the person is.
fn tally(screen: &Screen) -> &str {
    &screen.rows[0]
}

/// The row over the keys, where the run waiting longest on the person is named.
fn attention(screen: &Screen) -> &str {
    &screen.rows[screen.rows.len() - 2]
}

/// Waits until `shown` holds of what `terminal` shows, failing with `what` and the screen.
fn wait_until_shown(terminal: &Terminal, shown: impl Fn(&Screen) -> bool, what: &str) {
    wait_until(
        || shown(&terminal.screen()),
        || {
            format!(
                "{what}\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
}

/// The engine of the run `id` is a process of its own, not the application at `terminal`.
fn run_elsewhere(checkout: &Checkout, id: &str, terminal: &Terminal) {
    assert_ne!(
        engine_of(checkout, id),
        u64::from(terminal.process_id()),
        "the application follows the run and never runs it"
    );
}

/// The held run `held` on the field `yunta` opens, under what runs, its engine elsewhere.
fn on_the_field(held: &HeldRun) -> Terminal {
    let terminal = opened(&held.checkout);
    wait_until_shown(
        &terminal,
        |screen| group_of(screen, &held.id) == Some(RUNNING),
        "the run held never stood under what runs",
    );
    run_elsewhere(&held.checkout, &held.id, &terminal);
    terminal
}

/// The process the engine of the run `id` runs in, killed, once it is gone.
fn engine_killed(checkout: &Checkout, id: &str) {
    let engine = u32::try_from(engine_of(checkout, id)).expect("a process id");
    let engine = Pid::try_from(engine).expect("a process id");
    signal_process(engine, Signal::SIGKILL).expect("the engine is alive to be killed");
    wait_until(
        || liveness(engine) == Liveness::Dead,
        || "the killed engine never went".to_string(),
    );
}

/// Where the run `id` of `checkout` stands on its log: its last event's seq.
fn head(checkout: &Checkout, id: &str) -> u64 {
    let heads = heads(&checkout.home.join("yunta.db"));
    let (_, seq) = heads
        .into_iter()
        .find(|(run, _)| run.as_str() == id)
        .expect("the run on the log");
    seq.get()
}

#[test]
fn a_run_started_detached_comes_under_what_runs_then_parks_under_needs_you() {
    let hold = Hold::new();
    let checkout = repository(&hold);
    let done = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(done.status.success(), "{}", stderr(&done));
    let mut terminal = opened(&checkout);
    terminal.wait_on_screen("1 closed", "the field never counted its closed run");

    let id = started_detached(&checkout);
    wait_until_shown(
        &terminal,
        |screen| group_of(screen, &id) == Some(RUNNING),
        "the run started never came under what runs",
    );
    wait_until(|| hold.started(), || "the run never started".to_string());
    run_elsewhere(&checkout, &id, &terminal);

    hold.give(PARK);
    let named = format!("{} needs you", handle(&id));
    wait_until_shown(
        &terminal,
        |screen| {
            group_of(screen, &id) == Some(NEEDS_YOU)
                && tally(screen).contains("1 needs you")
                && attention(screen).contains(&named)
        },
        "the run parked never moved under needs you, counted and named",
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

#[test]
fn a_run_answered_from_a_shell_leaves_needs_you_then_goes_among_the_closed() {
    let hold = Hold::new();
    let checkout = repository(&hold);
    let id = started_detached(&checkout);
    hold.give(PARK);
    let mut terminal = opened(&checkout);
    wait_until_shown(
        &terminal,
        |screen| group_of(screen, &id) == Some(NEEDS_YOU) && tally(screen).contains("1 needs you"),
        "the run parked never stood under needs you",
    );

    let answered = yunta_at!(&checkout, &["resolve-gate", &id, "go"]);
    assert!(answered.status.success(), "{}", stderr(&answered));
    wait_until_shown(
        &terminal,
        |screen| {
            group_of(screen, &id) == Some(RUNNING)
                && !tally(screen).contains("needs you")
                && !attention(screen).contains("needs you")
        },
        "the run answered never left needs you",
    );
    wait_until(
        || hold.word(RESUMED).exists(),
        || "the run answered never went on".to_string(),
    );
    run_elsewhere(&checkout, &id, &terminal);

    hold.give(FINISH);
    wait_until_shown(
        &terminal,
        |screen| group_of(screen, &id).is_none() && screen.holds("1 closed"),
        "the run finished never went among the closed",
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
    assert_eq!(outcome(&checkout, &id), "finished");
}

#[test]
fn a_run_cancelled_from_a_shell_moves_under_what_is_stopped() {
    let held = HeldRun::started_detached(yunta());
    let mut terminal = on_the_field(&held);
    let cancelled = yunta_at!(&held.checkout, &["cancel", &held.id]);
    assert!(cancelled.status.success(), "{}", stderr(&cancelled));
    wait_until_shown(
        &terminal,
        |screen| {
            group_of(screen, &held.id) == Some("stopped") && tally(screen).contains("1 stopped")
        },
        "the run cancelled never moved under what is stopped",
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

#[test]
fn a_run_whose_engine_is_killed_moves_under_stalled_with_nothing_past_the_kill() {
    let held = HeldRun::started_detached(yunta());
    let mut terminal = on_the_field(&held);
    engine_killed(&held.checkout, &held.id);
    let at_the_kill = head(&held.checkout, &held.id);
    wait_until_shown(
        &terminal,
        |screen| {
            group_of(screen, &held.id) == Some("stalled") && tally(screen).contains("1 stalled")
        },
        "the run whose engine was killed never moved under stalled",
    );
    assert_eq!(
        head(&held.checkout, &held.id),
        at_the_kill,
        "nothing is on the log past the kill"
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

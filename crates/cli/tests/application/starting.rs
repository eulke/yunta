//! Starting a run in the application: the first run of a repository set up with no run,
//! what is asked for typed and its workflow and adapter picked; and from the field, a
//! workflow of the catalog started. Each run starts detached, an engine of its own that
//! the application follows and leaves moving, and `yunta list` lists it.

use yunta_testkit::workflows::{FINISHES, GREETS};
use yunta_testkit::{
    handle, run_ids, stdout, wait_until, yunta_at, yunta_on_terminal, Checkout, Hold, Terminal,
};

use super::{engine_of, given_back, opened, outcome, FINISHED, FOLLOWED};

/// What the first run of a repository asks.
const ASKED_FIRST: &str = "What do you want to build?";

/// What `terminal` asks for typed, `asked`, once it shows what was typed, then the adapter
/// its list opens on.
fn started_with(terminal: &mut Terminal, asked: &str) {
    terminal.keys(asked);
    terminal.wait_on_screen(&format!("> {asked}"), "what was typed never showed");
    terminal.keys("\r");
    terminal.wait_on_screen(
        "Which adapter runs its sessions?",
        "the adapter was never asked",
    );
    terminal.keys("\r");
}

/// Whether `yunta list` lists the run `id`, closed runs too.
fn listed(checkout: &Checkout, id: &str) -> bool {
    stdout(&yunta_at!(checkout, &["list", "--runs", "--full"])).contains(handle(id))
}

#[test]
fn the_first_run_typed_at_an_empty_field_starts_detached_and_is_followed_to_its_end() {
    let checkout = Checkout::new()
        .working_in_place()
        .file(".yunta/workflows/greet.yaml", GREETS)
        .committed();
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &[]);
    terminal.wait_on_screen(ASKED_FIRST, "the first run never asked");
    // What it asks is answered once the repository's workflows are read and shown under
    // it: an `enter` before that finds no workflow to go on with.
    terminal.wait_on_screen("yunta run greet", "the workflows were never read");
    terminal.keys("greet ana");
    terminal.wait_on_screen("> greet ana", "what was typed never showed");
    terminal.keys("\r");
    terminal.wait_on_screen("Which workflow builds it?", "the workflow was never asked");
    terminal.keys("\r");
    terminal.wait_on_screen(
        "Which adapter runs its sessions?",
        "the adapter was never asked",
    );
    terminal.keys("\r");
    terminal.wait_on_screen(FINISHED, "the run started was never followed to its end");
    let [id] = run_ids(&checkout.home).try_into().expect("one run started");
    assert!(listed(&checkout, &id), "`yunta list` lists it");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// From the field, `w` opens the catalog, and `enter` starts the workflow pointed at: what
/// it is asked for typed, its adapter picked. The run is an engine of its own, which goes
/// on once the application leaves.
#[test]
fn a_workflow_started_from_the_field_runs_detached_and_goes_on_once_the_application_leaves() {
    let hold = Hold::new();
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("done", FINISHES)
        .file(
            ".yunta/workflows/hold.yaml",
            &hold.workflow_taking_a_request(),
        )
        .committed();
    let finished = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(finished.status.success(), "the field's run finished");
    let mut terminal = opened(&checkout);
    // Bound again after the terminal, so it is dropped before it, however the test ends:
    // the held engine has to end for the terminal to close.
    let hold = hold;
    terminal.keys("w");
    terminal.wait_on_screen("hold", "the catalog never listed the workflow");
    terminal.keys("\r");
    terminal.wait_on_screen("ask (string, required)", "the start never asked its input");
    started_with(&mut terminal, "hold it");
    terminal.wait_on_screen(FOLLOWED, "the run started was never followed");
    wait_until(|| hold.started(), || "the run never started".to_string());
    let id = run_ids(&checkout.home)
        .into_iter()
        .find(|id| outcome(&checkout, id) == "running")
        .expect("the run started");
    assert_ne!(
        engine_of(&checkout, &id),
        u64::from(terminal.process_id()),
        "the run is an engine of its own"
    );
    terminal.keys("q");
    wait_until(
        || terminal.exited(),
        || "the application never left".to_string(),
    );
    assert_eq!(outcome(&checkout, &id), "running", "the run goes on");
    hold.let_go();
    wait_until(
        || outcome(&checkout, &id) == "finished",
        || format!("the run never finished: {}", outcome(&checkout, &id)),
    );
    // The engine stays in the terminal's session, as one `yunta run --detach` starts from
    // a shell does, so the terminal closes once it ended.
    assert_eq!(given_back(&mut terminal), Some(0));
    assert!(listed(&checkout, &id), "`yunta list` lists it");
}

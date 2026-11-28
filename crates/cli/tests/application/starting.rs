//! Starting a run in the application, by every way into a new run: the first run of a
//! repository set up with no run, what is asked for typed at the first request and started
//! from the new run it goes on to, every other row as it opens; from the field, `n` and a
//! workflow, its mode and an adapter chosen on the new run's rows; and from the workflows, a
//! workflow of the catalog started from its new run, what it is asked for typed there. Each
//! run starts detached, an engine of its own that the application follows and leaves moving;
//! `yunta list` lists it, and it was born with what the new run's rows held.
//!
//! Each run's first node holds until its test lets it go, and ends by itself once the test's
//! hold is dropped, which is before its terminal however the test ends: the run then
//! finishes, its engine ends and the terminal it held closes, so no run outlives its test.

use serde_json::Value;
use yunta_core::{Manifest, RunAdapter};
use yunta_render::page::NewRun;
use yunta_render::screen::Leaving;
use yunta_testkit::workflows::{FINISHES, GATED, GREETS};
use yunta_testkit::{
    handle, run_ids, runs_root, stderr, stdout, wait_for, wait_until, yunta_at, yunta_on_terminal,
    Checkout, Hold, Terminal,
};

use super::{engine_of, given_back, left_through_the_field, opened, outcome, FOLLOWED, THE_FIELD};

/// What the first run of a repository asks.
const ASKED_FIRST: &str = "What do you want to build?";

/// What each run is asked for, typed as its request.
const REQUEST: &str = "hold it";

/// The key that moves a new run's focus to its next row.
const TAB: &str = "\t";

/// The key that moves a pointer down its list.
const DOWN: &str = "\u{1b}[B";

/// The key that chooses the next word of a row of words.
const RIGHT: &str = "\u{1b}[C";

/// `words` typed on the row `terminal` types on, once its box shows them.
fn typed(terminal: &mut Terminal, words: &str) {
    terminal.keys(words);
    terminal.wait_on_screen(&format!("│ {words}"), "what was typed never showed");
}

/// Whether `yunta list` lists the run `id`, closed runs too.
fn listed(checkout: &Checkout, id: &str) -> bool {
    stdout(&yunta_at!(checkout, &["list", "--runs", "--full"])).contains(handle(id))
}

/// `hold`'s workflow taking a request, with two modes that each run it whole: `quick`, its
/// first, and `full`. The mode chosen changes the run's mode and nothing it runs.
fn held_in_two_modes(hold: &Hold) -> String {
    format!(
        "{}modes:\n  quick: {{ include: all }}\n  full: {{ include: all }}\n",
        hold.workflow_taking_a_request()
    )
}

/// What a run was started with: its workflow, its mode, its request, and the adapter every
/// session of it is pinned to, none where each runner's own adapter runs it.
#[derive(Debug, PartialEq, Eq)]
struct Chosen {
    workflow: String,
    mode: String,
    request: Option<String>,
    adapter: Option<String>,
}

impl Chosen {
    /// A run of `workflow` in `mode`, asked for with `request`, on `adapter`.
    fn asked(workflow: &str, mode: &str, request: &str, adapter: Option<&str>) -> Chosen {
        Chosen {
            workflow: workflow.to_string(),
            mode: mode.to_string(),
            request: Some(request.to_string()),
            adapter: adapter.map(str::to_string),
        }
    }

    /// The run `id` of `checkout` as it was born: its workflow and its mode as `yunta status`
    /// prints them, its request and its adapter as the manifest it froze holds them.
    fn of(checkout: &Checkout, id: &str) -> Chosen {
        let status = yunta_at!(checkout, &["status", id, "--json"]);
        let document: Value = serde_json::from_str(&stdout(&status)).expect("one document");
        let said = |field: &str| document[field].as_str().unwrap_or_default().to_string();
        let frozen = std::fs::read(runs_root(&checkout.home).join(id).join("manifest.yaml"))
            .expect("the run's manifest");
        let manifest = Manifest::read_frozen(&frozen)
            .expect("a manifest that reads")
            .doc;
        Chosen {
            workflow: said("workflow"),
            mode: said("mode"),
            request: manifest.request().map(str::to_string),
            adapter: manifest
                .adapter
                .as_ref()
                .and_then(RunAdapter::pinned)
                .map(|adapter| adapter.as_str().to_string()),
        }
    }
}

/// The run of `checkout` that `yunta list` lists and none of `before` is: the one a new run
/// started, once the list holds it.
fn listed_beside(checkout: &Checkout, before: &[String]) -> Option<String> {
    let runs = match runs_root(&checkout.home).is_dir() {
        true => run_ids(&checkout.home),
        false => Vec::new(),
    };
    runs.into_iter()
        .find(|id| !before.contains(id))
        .filter(|id| listed(checkout, id))
}

/// The run the new run at `terminal` started in `checkout` once `enter` asked for it, beside
/// the runs `before` names: listed by `yunta list`, then followed, its first node started and
/// held by `hold`, an engine of its own. Its whole id.
fn started_and_followed(
    terminal: &Terminal,
    checkout: &Checkout,
    hold: &Hold,
    before: &[String],
) -> String {
    let id = wait_for(
        || listed_beside(checkout, before),
        || {
            let list = yunta_at!(checkout, &["list", "--runs", "--full"]);
            format!(
                "no run is listed but those before ({before:?}):\n{}\non the screen:\n{}",
                stdout(&list),
                terminal.screen().rows.join("\n")
            )
        },
    );
    terminal.wait_on_screen(FOLLOWED, "the run started was never followed");
    wait_until(|| hold.started(), || "the run never started".to_string());
    assert_ne!(
        engine_of(checkout, &id),
        u64::from(terminal.process_id()),
        "the run is an engine of its own"
    );
    id
}

/// `terminal`, following the run `id` of `checkout` while `hold` holds it, left through the
/// field: the application leaves and the run goes on. Let go, the run finishes, its engine
/// ends and the terminal it held closes, given back as the binary found it; `yunta list`
/// still lists the run, among the closed.
fn left_going_on(terminal: &mut Terminal, checkout: &Checkout, hold: &Hold, id: &str) {
    left_through_the_field(terminal, Some(Leaving::KeepsGoing));
    wait_until(
        || terminal.exited(),
        || "the application never left".to_string(),
    );
    assert_eq!(outcome(checkout, id), "running", "the run goes on");
    hold.let_go();
    wait_until(
        || outcome(checkout, id) == "finished",
        || format!("the run never finished: {}", outcome(checkout, id)),
    );
    // The engine stays in the terminal's session, as one `yunta run --detach` starts from
    // a shell does, so the terminal closes once it ended.
    assert_eq!(given_back(terminal), Some(0));
    assert!(listed(checkout, id), "`yunta list` lists it");
}

/// The first run of a repository set up with no run: the request typed at the first request
/// and `enter` open the new run with the request in its line, and `enter` there starts the run
/// with every other row as it opens: the one workflow, its mode, and each session on its
/// runner's own adapter. The run is an engine of its own, which goes on once the application
/// leaves.
#[test]
fn the_first_run_typed_at_the_first_request_runs_detached_and_goes_on_once_the_application_leaves()
{
    let hold = Hold::new();
    let checkout = Checkout::new()
        .working_in_place()
        .file(
            ".yunta/workflows/hold.yaml",
            &hold.workflow_taking_a_request(),
        )
        .committed();
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &[]);
    // Bound again after the terminal, so it is dropped before it, however the test ends:
    // the held engine has to end for the terminal to close.
    let hold = hold;
    terminal.wait_on_screen(ASKED_FIRST, "the first run never asked");
    typed(&mut terminal, REQUEST);
    terminal.keys("\r");
    // The new run reads the workflows as it opens: what the chosen one runs stands once they
    // are read, and an `enter` before that finds no workflow to start.
    terminal.wait_on_screen(NewRun::WHAT_RUNS, "the new run never read its workflows");
    let screen = terminal.screen();
    assert!(
        screen.holds(&format!("│ {REQUEST}")),
        "the new run holds the request in its line:\n{}",
        screen.rows.join("\n")
    );
    terminal.keys("\r");
    let id = started_and_followed(&terminal, &checkout, &hold, &[]);
    assert_eq!(
        Chosen::of(&checkout, &id),
        Chosen::asked("hold", "default", REQUEST, None),
        "the run is the one the rows held"
    );
    left_going_on(&mut terminal, &checkout, &hold, &id);
}

/// Words typed on the first request stand while a run started from a shell parks on the
/// person: the tally counts the run before the next key, the question and the words stay,
/// and `esc` then goes to the field, which holds the run. The run's own `yunta run` ends as
/// the run parks, so nothing it started outlives the test.
#[test]
fn the_first_request_stays_under_a_typist_while_a_run_starts_elsewhere() {
    let checkout = Checkout::new()
        .working_in_place()
        .file(".yunta/workflows/greet.yaml", GREETS)
        .workflow("wf", GATED)
        .committed();
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &[]);
    terminal.wait_on_screen(ASKED_FIRST, "the first run never asked");
    terminal.keys("greet ana");
    terminal.wait_on_screen("│ greet ana", "what was typed never showed");
    let parked = yunta_at!(&checkout, &["run", "wf.yaml"]);
    assert_eq!(
        parked.status.code(),
        Some(3),
        "it parks: {}",
        stderr(&parked)
    );
    // The tally stands at the trail's row; its words give way to a long path, its mark and
    // its count never do.
    let tallied = || {
        let screen = terminal.screen();
        screen.rows.first().is_some_and(|row| row.contains("◆ 1"))
    };
    wait_until(tallied, || {
        let rows = terminal.screen().rows.join("\n");
        format!("the tally never counted the run started elsewhere:\n{rows}")
    });
    // The run is counted as soon as its gate is in the log, and named on the attention row
    // once the application has read that the engine that asked at its own terminal is gone:
    // a step later, so the row is waited on.
    terminal.wait_on_screen(
        "needs you: Go on?",
        "the attention row never named the run started elsewhere",
    );
    let screen = terminal.screen();
    let rows = screen.rows.join("\n");
    assert!(
        screen.holds(ASKED_FIRST) && screen.holds("│ greet ana"),
        "the question and the words typed stand:\n{rows}"
    );
    assert!(
        !screen.holds("go there"),
        "the attention row names the run with no key, since every key types:\n{rows}"
    );
    terminal.keys("\u{1b}");
    terminal.wait_on_screen(THE_FIELD, "`esc` never went to the field");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// From the field, `n` opens a new run; its request typed, then a workflow chosen other than
/// the one it opens on, that workflow's mode other than its first, and an adapter in place of
/// each runner's own, `enter` starts the run every row asked for. The run is an engine of its
/// own, which goes on once the application leaves.
#[test]
fn a_new_run_from_the_field_starts_what_its_rows_chose_and_goes_on_once_the_application_leaves() {
    let hold = Hold::new();
    let adapter = yunta_api::machine::first_built_adapter();
    // The runners name the adapter chosen, as admission asks of a run pinned to one; the
    // workflows run no session, so none is opened on it.
    let config = format!(
        "defaults:\n  isolation: none\nrunners:\n  executor:\n    - {{ adapter: {adapter}, model: any }}\n"
    );
    let checkout = Checkout::new()
        .config(&config)
        .workflow("done", FINISHES)
        .file(".yunta/workflows/greet.yaml", GREETS)
        .file(".yunta/workflows/hold.yaml", &held_in_two_modes(&hold))
        .committed();
    let finished = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(finished.status.success(), "the field's run finished");
    let before = run_ids(&checkout.home);
    let mut terminal = opened(&checkout);
    // Bound again after the terminal, so it is dropped before it, however the test ends:
    // the held engine has to end for the terminal to close.
    let hold = hold;
    terminal.keys("n");
    // The new run reads the workflows as it opens: its workflow's row stands once they are
    // read, and the keys reach it only then.
    terminal.wait_on_screen(NewRun::WHAT_RUNS, "the new run never read its workflows");
    typed(&mut terminal, REQUEST);
    terminal.keys(TAB);
    terminal.keys(DOWN);
    terminal.wait_on_screen("▸ hold", "the workflow was never chosen");
    terminal.keys(TAB);
    terminal.keys(RIGHT);
    terminal.wait_on_screen("▸ full", "the mode was never chosen");
    terminal.keys(TAB);
    terminal.keys(RIGHT);
    terminal.wait_on_screen(&format!("▸ {adapter}"), "the adapter was never chosen");
    terminal.keys("\r");
    let id = started_and_followed(&terminal, &checkout, &hold, &before);
    assert_eq!(
        Chosen::of(&checkout, &id),
        Chosen::asked("hold", "full", REQUEST, Some(adapter.as_str())),
        "the run is the one the rows held"
    );
    left_going_on(&mut terminal, &checkout, &hold, &id);
}

/// From the field, `w` opens the workflows, and `enter` on one other than the first opens a
/// new run of it, which starts with what it is asked for typed, every other row as it opens:
/// that workflow's mode, and each session on its runner's own adapter. The run is an engine
/// of its own, which goes on once the application leaves.
#[test]
fn a_workflow_started_from_the_workflows_runs_detached_and_goes_on_once_the_application_leaves() {
    let hold = Hold::new();
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("done", FINISHES)
        .file(".yunta/workflows/greet.yaml", GREETS)
        .file(
            ".yunta/workflows/hold.yaml",
            &hold.workflow_taking_a_request(),
        )
        .committed();
    let finished = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(finished.status.success(), "the field's run finished");
    let before = run_ids(&checkout.home);
    let mut terminal = opened(&checkout);
    // Bound again after the terminal, so it is dropped before it, however the test ends:
    // the held engine has to end for the terminal to close.
    let hold = hold;
    terminal.keys("w");
    terminal.wait_on_screen("▸ greet", "the catalog never listed the workflows");
    terminal.keys("j");
    terminal.wait_on_screen("▸ hold", "the catalog never pointed at the workflow");
    terminal.keys("\r");
    // The new run reads the workflows as it opens: what the chosen one runs stands once they
    // are read, and an `enter` before that finds no workflow to start.
    terminal.wait_on_screen(NewRun::WHAT_RUNS, "the new run never read its workflows");
    typed(&mut terminal, REQUEST);
    terminal.keys("\r");
    let id = started_and_followed(&terminal, &checkout, &hold, &before);
    assert_eq!(
        Chosen::of(&checkout, &id),
        Chosen::asked("hold", "default", REQUEST, None),
        "the run is the one the rows held"
    );
    left_going_on(&mut terminal, &checkout, &hold, &id);
}

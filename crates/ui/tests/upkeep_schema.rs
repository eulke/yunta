//! Upkeep and the shape of a document, in the application. Upkeep, opened from doctor:
//! what collecting would reclaim, as `yunta gc --dry-run` prints it, then, once the
//! person confirms it, the collection, as `yunta gc` prints it, what it remarks shown as
//! it comes; doctor checks again on the way back. The shapes, opened from the
//! workflows: every document Yunta reads, one picked by typing, shown as `yunta schema
//! <name>` prints it.

mod support;

use std::path::Path;
use std::sync::Arc;

use surco::components::Note;
use surco::doc::Doc;
use surco::surface::{Markdown, Surface};
use yunta_api::Remark;
use yunta_core::DocumentKind;
use yunta_render::page::doctor::{CheckoutsKeep, Finding};
use yunta_render::page::{Collected, Page, Schema};
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State, Ticket};

use support::{drive, on_the_field, performed, stopped, workspace};

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// `page` as Markdown writes it.
fn drawn(page: &impl Page) -> String {
    let look = Markdown.look();
    Markdown.draw(&page.document(&look))
}

/// Where the person is.
fn trail(state: &State) -> Vec<String> {
    state.screen(&Markdown.look()).trail
}

/// The application over a field of one stopped run.
fn on_a_field() -> State {
    let run = "01M7UPKEEP0000000000000000".parse().expect("a run's id");
    on_the_field(vec![stopped(&run)])
}

/// The state root of `bench`'s repository: where its runs and log are.
fn root_of(bench: &Bench) -> &Path {
    bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root")
}

/// Upkeep opened from doctor, its checkouts over their budget, standing on what
/// collecting would reclaim in `bench`: the state, and that dry run's page.
async fn at_upkeep(bench: &Bench) -> (State, Box<Collected>) {
    let mut state = on_a_field();
    let asked = state.update(Input::Stroke(Stroke::Insert('m')));
    let [Request::Perform(checks, Operation::Doctor { sessions: false })] = asked.as_slice() else {
        panic!("doctor checks the machine as it opens: {asked:?}");
    };
    let over = vec![Finding::Checkouts {
        kept: "4 checkouts".to_string(),
        keep: CheckoutsKeep::BuildsOverBudget,
    }];
    state.update(Input::Outcome(*checks, Outcome::Doctor(over)));
    let asked = state.update(Input::Stroke(Stroke::Insert('u')));
    let [Request::Perform(dry, Operation::Collect { dry_run: true })] = asked.as_slice() else {
        panic!("upkeep opens on a dry run: {asked:?}");
    };
    let (_, outcome) = performed(bench, Operation::Collect { dry_run: true }).await;
    let Outcome::Collected(would) = &outcome else {
        panic!("a dry run comes to what it would reclaim: {outcome:?}");
    };
    let would = would.clone();
    state.update(Input::Outcome(*dry, outcome));
    (state, would)
}

/// `state`, at upkeep, asked to collect and confirmed: the ticket the collection runs
/// under.
fn collecting(state: &mut State) -> Ticket {
    let asked = state.update(Input::Stroke(Stroke::Insert('c')));
    assert!(
        asked.is_empty(),
        "nothing is collected until confirmed: {asked:?}"
    );
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(pass, Operation::Collect { dry_run: false })] = asked.as_slice() else {
        panic!("confirmed, the pass collects: {asked:?}");
    };
    *pass
}

#[tokio::test]
async fn upkeep_opens_from_doctor_on_what_collecting_would_reclaim() {
    let bench = Bench::new();
    let (state, would) = at_upkeep(&bench).await;
    assert_eq!(main_region(&state), drawn(&*would));
    assert_eq!(trail(&state), ["yunta", "upkeep"]);
}

#[tokio::test]
async fn what_runs_left_is_collected_once_confirmed_its_remarks_above_its_page() {
    let bench = Bench::new();
    let (mut state, _) = at_upkeep(&bench).await;
    state.update(Input::Stroke(Stroke::Insert('c')));
    let overlay = state.screen(&Markdown.look()).overlay;
    let asks = overlay.map(|overlay| Markdown.draw(&overlay.doc));
    assert!(
        asks.as_deref()
            .is_some_and(|asks| asks.contains("removed for good")),
        "{asks:?}"
    );
    state.update(Input::Stroke(Stroke::Decline));
    let pass = collecting(&mut state);
    let said = "a checkout of 01M7 is still held".to_string();
    state.update(Input::Remark(
        pass,
        Remark::Unstopped { said: said.clone() },
    ));
    let (_, outcome) = performed(&bench, Operation::Collect { dry_run: false }).await;
    let Outcome::Collected(collected) = &outcome else {
        panic!("the pass comes to what it reclaimed: {outcome:?}");
    };
    state.update(Input::Outcome(pass, outcome.clone()));
    // What it said as it went stands above the page, as `yunta gc` says it before it.
    let page = collected.document(&Markdown.look()).into_blocks();
    let shown = Doc::new().with(Note(said)).with_all(page);
    assert_eq!(main_region(&state), Markdown.draw(&shown));
}

#[tokio::test]
async fn escape_stops_a_collection_and_doctor_checks_again_on_the_way_back() {
    let bench = Bench::new();
    let (mut state, _) = at_upkeep(&bench).await;
    let pass = collecting(&mut state);
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(
        asked
            .iter()
            .any(|request| matches!(request, Request::LetGo(ticket) if *ticket == pass)),
        "{asked:?}"
    );
    // Part of what the dry run listed may be gone: it is found again.
    assert!(
        asked.iter().any(|request| matches!(
            request,
            Request::Perform(_, Operation::Collect { dry_run: true })
        )),
        "{asked:?}"
    );
    let shown = main_region(&state);
    assert!(
        shown.contains(yunta_render::screen::COLLECTION_STOPPED),
        "{shown}"
    );
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(
        asked.iter().any(|request| matches!(
            request,
            Request::Perform(_, Operation::Doctor { sessions: false })
        )),
        "{asked:?}"
    );
    assert_eq!(trail(&state), ["yunta", "doctor"]);
}

#[tokio::test]
async fn a_documents_shape_is_picked_by_typing_and_shown_as_schema_prints_it() {
    let bench = Bench::new();
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let strokes = [Stroke::Insert('w'), Stroke::Insert('s')];
    let (mut state, _) = drive(on_a_field(), Vec::new(), &performer, &strokes).await;
    assert_eq!(trail(&state), ["yunta", "schema"]);
    for stroke in ["t", "a", "s"].map(|typed| Stroke::Insert(typed.chars().next().unwrap())) {
        state.update(Input::Stroke(stroke));
    }
    state.update(Input::Stroke(Stroke::Enter));
    let tasks: DocumentKind = "tasks".parse().expect("a document Yunta reads");
    assert_eq!(main_region(&state), drawn(&Schema::of(tasks)));
    assert_eq!(trail(&state), ["yunta", "schema", "tasks"]);
    // Escape goes back to the documents, the one picked among them still, then to the
    // workflows.
    state.update(Input::Stroke(Stroke::Decline));
    assert_eq!(trail(&state), ["yunta", "schema"]);
    assert!(
        main_region(&state).contains("tasks"),
        "{}",
        main_region(&state)
    );
    state.update(Input::Stroke(Stroke::Decline));
    assert_eq!(trail(&state), ["yunta", "workflows"]);
}

//! A new run: one screen holding a row for each thing a run is started with, the request, the
//! workflow, each other input it declares, its mode and its adapter, the keys on one row at a
//! time; what the rows hold is the run `yunta run` asks for with the same choices. Going back
//! keeps what was typed: in the first request's line where the new run was opened there, else
//! in the new run itself, which the next new run opens as it was left until a run starts, as
//! `again` holds. The adapters its adapter row offers are `adapters`'.

#[path = "new_run/adapters.rs"]
mod adapters;
#[path = "new_run/again.rs"]
mod again;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::Markdown;
use yunta_api::RunRequest;
use yunta_core::events::Channel;
use yunta_core::{AdapterId, InputName, ModeName, RunAdapter, RunId};
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::{drive, driven};
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State, Ticket};

/// A workflow that takes what was asked for as its request and whom to greet, in either of
/// two modes: the first runs its greeting alone, the second stops for a person after it.
const GREETS: &str = "name: greet\ninputs:\n  ask: { type: string, request: true }\n  \
                      who: { type: string, description: \"whom it greets\" }\nmodes:\n  \
                      quick: { include: [say] }\n  full: { include: all }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n  \
                      - { id: approve, kind: gate, assignee: lead, message: \"Go on?\", \
                      options: [go], depends_on: [say] }\n";

/// A mock fixture with no sessions: the run beside the new run opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The run the field holds, so the application stands on the field.
const HELD: RunId = RunId::from_static("01M7NEWRVN000000000000KEPT");

/// The config of a repository whose sessions work in place, its runners naming no adapter.
const IN_PLACE: &str = "defaults:\n  isolation: none\n";

/// The config of a repository whose sessions work in place, its one runner with a candidate
/// on each of `adapters`.
fn running_on(adapters: &[&AdapterId]) -> String {
    let candidates: String = adapters
        .iter()
        .map(|adapter| format!("    - {{ adapter: {adapter}, model: any }}\n"))
        .collect();
    format!("{IN_PLACE}runners:\n  executor:\n{candidates}")
}

/// `repo`, a git repository, set up for Yunta with `greet` in its catalog, its runners naming
/// no adapter.
fn set_up_in(repo: &Path) {
    set_up_with(repo, IN_PLACE);
}

/// `repo`, a git repository, set up for Yunta with `config` and `greet` in its catalog.
fn set_up_with(repo: &Path, config: &str) {
    std::fs::create_dir_all(repo.join(".yunta/workflows")).unwrap();
    std::fs::write(repo.join(".yunta/config.yaml"), config).unwrap();
    std::fs::write(repo.join(".yunta/workflows/greet.yaml"), GREETS).unwrap();
}

/// Each character of `words` typed.
fn typed(words: &str) -> Vec<Stroke> {
    words.chars().map(Stroke::Insert).collect()
}

/// The application opened on a repository set up under `root` with no run, its runners naming
/// no adapter, standing on the first request with the catalog read, and the performer that
/// works there.
async fn on_the_first_request(root: &Path) -> (State, Arc<Performer>) {
    on_the_first_request_with(root, IN_PLACE).await
}

/// The application opened on a repository set up under `root` with `config` and no run,
/// standing on the first request with the catalog read, and the performer that works there.
async fn on_the_first_request_with(root: &Path, config: &str) -> (State, Arc<Performer>) {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    set_up_with(&repo, config);
    let performer = Arc::new(Performer::new(workspace(&repo, root), Channel::Tty));
    let (opened, asked) = State::opened(now());
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    (state, performer)
}

/// `strokes` handed to `state` one after the other, what the last asked.
fn pressed(state: &mut State, strokes: &[Stroke]) -> Vec<Request> {
    let mut asked = Vec::new();
    for stroke in strokes {
        asked = state.update(Input::Stroke(*stroke));
    }
    asked
}

/// What `state` draws, as Markdown.
fn drawn(state: &State) -> String {
    Markdown.screen(&state.screen(&Markdown.look()))
}

/// The request typed at the first request, then each row reached with `tab`: whom to greet
/// typed, the mode moved on with `→`, the adapter moved on to the one the runners name, the
/// first this binary builds; `enter` asks for the run `yunta run greet --input ask=… --input
/// who=… --mode full --adapter …` asks for, field by field.
#[tokio::test]
async fn the_rows_ask_for_the_run_yunta_run_asks_for_with_the_same_choices() {
    let first_built = yunta_api::machine::built_adapter_ids()
        .into_iter()
        .next()
        .expect("this binary builds an adapter");
    let root = tempfile::tempdir().unwrap();
    let config = running_on(&[&first_built]);
    let (state, performer) = on_the_first_request_with(root.path(), &config).await;
    let mut strokes = typed("greet ana");
    strokes.extend([Stroke::Enter, Stroke::Tab, Stroke::Tab]);
    strokes.extend(typed("bob"));
    strokes.extend([Stroke::Tab, Stroke::Right, Stroke::Tab, Stroke::Right]);
    let (mut state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some("new run"));

    let asked = pressed(&mut state, &[Stroke::Enter]);
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the run is asked for: {asked:?}");
    };
    let yunta_run = RunRequest {
        workflow: PathBuf::from("greet"),
        inputs: HashMap::from([
            (InputName::from("ask"), "greet ana".to_string()),
            (InputName::from("who"), "bob".to_string()),
        ]),
        mode: Some(ModeName::from("full")),
        adapter: Some(RunAdapter::Named(first_built)),
        hermetic: false,
    };
    assert_eq!(request.workflow, yunta_run.workflow);
    assert_eq!(request.inputs, yunta_run.inputs);
    assert_eq!(request.mode, yunta_run.mode);
    assert_eq!(request.adapter, yunta_run.adapter);
    assert_eq!(request.hermetic, yunta_run.hermetic);
}

/// What the workflow and the adapter open on, with what was asked for and nothing else
/// chosen, asks for the run `yunta run greet --input ask='greet ana' --mode quick` asks for:
/// the workflow's first mode, and the adapter its runners name.
#[tokio::test]
async fn what_the_rows_open_on_asks_for_the_run_yunta_run_asks_for_with_none_named() {
    let root = tempfile::tempdir().unwrap();
    let (state, performer) = on_the_first_request(root.path()).await;
    let mut strokes = typed("greet ana");
    strokes.push(Stroke::Enter);
    let (mut state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    let asked = pressed(&mut state, &[Stroke::Enter]);
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the run is asked for: {asked:?}");
    };
    let yunta_run = RunRequest {
        workflow: PathBuf::from("greet"),
        inputs: HashMap::from([(InputName::from("ask"), "greet ana".to_string())]),
        mode: Some(ModeName::from("quick")),
        adapter: None,
        hermetic: false,
    };
    assert_eq!(request, &yunta_run);
}

/// `esc` on a new run opened from the first request goes back to it with the request as it
/// was edited on the new run, in its line.
#[tokio::test]
async fn going_back_from_the_first_requests_new_run_puts_the_edited_request_in_its_line() {
    let root = tempfile::tempdir().unwrap();
    let (mut state, _) = on_the_first_request(root.path()).await;
    let mut strokes = typed("greet ana");
    strokes.push(Stroke::Enter);
    strokes.extend(typed(" now"));
    pressed(&mut state, &strokes);
    assert!(drawn(&state).contains("greet ana now"), "{}", drawn(&state));

    pressed(&mut state, &[Stroke::Decline]);
    let screen = state.screen(&Markdown.look());
    assert_ne!(screen.trail.last().map(String::as_str), Some("new run"));
    let back = drawn(&state);
    assert!(back.contains("What do you want to build?"), "{back}");
    assert!(back.contains("greet ana now"), "the edited request: {back}");
    let asked = pressed(&mut state, &[Stroke::Enter]);
    assert!(
        matches!(
            asked.as_slice(),
            [
                Request::Perform(_, Operation::ReadCatalog),
                Request::Perform(_, Operation::Doctor { sessions: false })
            ]
        ),
        "the new run reads what it needs as it opens, and nothing more: {asked:?}"
    );
    assert!(drawn(&state).contains("greet ana now"), "{}", drawn(&state));
}

/// A new run opened over the field and left with `esc` is kept as it was left: the next new
/// run opens on it, words typed and all, until a run starts, after which a new run opens
/// with nothing typed.
#[tokio::test]
async fn a_new_run_left_unstarted_is_found_again_until_a_run_starts() {
    let world = Bench::with_run_id(HELD.as_str());
    set_up_in(&world.worktree);
    world.run(FINISHES, NO_SESSIONS).await;
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&world.worktree, root),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    let mut from_the_palette = vec![Stroke::Insert('/')];
    from_the_palette.extend(typed("new run"));
    from_the_palette.push(Stroke::Enter);
    let (mut state, _) = drive(state, Vec::new(), &performer, &from_the_palette).await;
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(
        trail.last().map(String::as_str),
        Some("new run"),
        "{trail:?}"
    );

    pressed(&mut state, &typed("kept words"));
    pressed(&mut state, &[Stroke::Decline]);
    assert!(!drawn(&state).contains("kept words"), "{}", drawn(&state));
    let (mut state, _) = drive(state, Vec::new(), &performer, &from_the_palette).await;
    assert!(drawn(&state).contains("kept words"), "{}", drawn(&state));

    let asked = pressed(&mut state, &[Stroke::Enter]);
    let Some(ticket) = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Start { .. }) => Some(*ticket),
        _ => None,
    }) else {
        panic!("the run is asked for: {asked:?}");
    };
    state.update(Input::Outcome(ticket, Outcome::Started { run: HELD }));
    pressed(&mut state, &[Stroke::Insert('q')]);
    let (state, _) = drive(state, Vec::new(), &performer, &from_the_palette).await;
    let blank = drawn(&state);
    assert!(blank.contains("new run"), "{blank}");
    assert!(!blank.contains("kept words"), "{blank}");
}

/// A workflow of one step, which finishes having spent nothing.
const ZETA: &str = "name: zeta\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

/// The ids of the three runs of `zeta` the repository holds, which finished.
const ZETAS: [&str; 3] = [
    "01M7NEWRVN0000000000ZETA01",
    "01M7NEWRVN0000000000ZETA02",
    "01M7NEWRVN0000000000ZETA03",
];

/// The new run's panel as Markdown draws `state`'s screen: the region beside the rows, which
/// Markdown says first, up to the rows' own first heading.
fn panel(state: &State) -> String {
    let drawn = drawn(state);
    let rows = drawn
        .find("What do you want to build?")
        .unwrap_or(drawn.len());
    drawn[..rows].to_string()
}

/// A repository that ran `zeta` three times and `greet` never, the application open on its
/// field, and the performer that works there: the world stays with them.
async fn on_a_field_that_ran_zeta() -> (State, Arc<Performer>, Bench) {
    let world = Bench::with_run_id(ZETAS[0]);
    set_up_in(&world.worktree);
    std::fs::write(world.worktree.join(".yunta/workflows/zeta.yaml"), ZETA).unwrap();
    world.run(ZETA, NO_SESSIONS).await;
    for id in &ZETAS[1..] {
        world.beside(id).run(ZETA, NO_SESSIONS).await;
    }
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&world.worktree, root),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    (state, performer, world)
}

/// The palette opened over the field, `new run` typed and taken.
fn new_run_from_the_palette() -> Vec<Stroke> {
    let mut strokes = vec![Stroke::Insert('/')];
    strokes.extend(typed("new run"));
    strokes.push(Stroke::Enter);
    strokes
}

/// A new run lists `zeta`, the workflow that ran most here, first and chosen, its panel saying
/// its three past runs and their median; moving to `greet` says its none, and moving back says
/// `zeta`'s again, each from the catalog's reading.
#[tokio::test]
async fn a_new_run_lists_what_ran_most_first_and_says_each_workflows_past_runs() {
    let (state, performer, _world) = on_a_field_that_ran_zeta().await;
    let opened = driven(state, Vec::new(), &performer, &new_run_from_the_palette()).await;
    let zeta = panel(&opened.state);
    assert!(zeta.contains("3 past runs"), "{zeta}");
    assert!(zeta.contains("median 0 tokens, p90 0"), "{zeta}");
    let listed = drawn(&opened.state);
    let (first, second) = (listed.find("zeta"), listed.find("greet"));
    let most_first = "the workflow that ran most is listed first";
    assert!(first < second, "{most_first}: {listed}");

    let to_greet = [Stroke::Tab, Stroke::Down];
    let moved = driven(opened.state, Vec::new(), &performer, &to_greet).await;
    let greet = panel(&moved.state);
    assert!(greet.contains("0 past runs"), "{greet}");
    assert!(!greet.contains("median"), "{greet}");
    let back = driven(moved.state, Vec::new(), &performer, &[Stroke::Up]).await;
    let zeta_again = panel(&back.state);
    assert!(zeta_again.contains("3 past runs"), "{zeta_again}");
}

/// Opening a new run reads the catalog once and what doctor finds once, its sessions
/// unopened; moving from workflow to workflow reads nothing, no history among it.
#[tokio::test]
async fn a_new_run_reads_the_catalog_and_the_machine_once_and_no_history() {
    let (state, performer, _world) = on_a_field_that_ran_zeta().await;
    let opened = driven(state, Vec::new(), &performer, &new_run_from_the_palette()).await;
    let walked = [Stroke::Tab, Stroke::Down, Stroke::Up, Stroke::Down];
    let moved = driven(opened.state, Vec::new(), &performer, &walked).await;
    let performed: Vec<&Operation> = opened.performed.iter().chain(&moved.performed).collect();
    let count = |kind: fn(&Operation) -> bool| performed.iter().filter(|op| kind(op)).count();
    let catalogs = count(|operation| matches!(operation, Operation::ReadCatalog));
    let doctors = count(|operation| matches!(operation, Operation::Doctor { sessions: false }));
    let histories = count(|operation| matches!(operation, Operation::ReadHistory { .. }));
    assert_eq!((catalogs, doctors, histories), (1, 1, 0), "{performed:?}");
}

/// The ticket `asked` performs `operation` under, where it does.
fn ticket_of(asked: &[Request], operation: impl Fn(&Operation) -> bool) -> Option<Ticket> {
    asked.iter().find_map(|request| match request {
        Request::Perform(ticket, asked) if operation(asked) => Some(*ticket),
        _ => None,
    })
}

/// What doctor finds of the machine, failed while the run asked for is still being started:
/// said in the note's place, the start still waited on, so the run it starts is followed in
/// the new run's place.
#[tokio::test]
async fn a_reading_that_fails_while_a_run_starts_leaves_the_start_waited_on() {
    let root = tempfile::tempdir().unwrap();
    let (mut state, performer) = on_the_first_request(root.path()).await;
    let mut strokes = typed("greet ana");
    strokes.push(Stroke::Enter);
    let opening = pressed(&mut state, &strokes);
    let machine = ticket_of(&opening, |operation| {
        matches!(operation, Operation::Doctor { sessions: false })
    });
    let machine = machine.expect("the new run asks what doctor finds as it opens");
    let catalog = ticket_of(&opening, |operation| {
        matches!(operation, Operation::ReadCatalog)
    });
    let catalog = catalog.expect("the new run asks the catalog as it opens");
    let read = performer.workspace().catalog().await;
    state.update(Input::Outcome(catalog, Outcome::Catalog(Box::new(read))));
    let asked = pressed(&mut state, &[Stroke::Enter]);
    let start = ticket_of(&asked, |operation| {
        matches!(operation, Operation::Start { .. })
    });
    let start = start.expect("the run is asked for");

    let failed = "doctor could not read the project".to_string();
    state.update(Input::Outcome(machine, Outcome::Failed(failed.clone())));
    let noted = state.screen(&Markdown.look()).noted.map(|noted| noted.said);
    assert_eq!(noted, Some(failed));
    state.update(Input::Outcome(start, Outcome::Started { run: HELD }));
    let trail = state.screen(&Markdown.look()).trail;
    assert!(
        trail.iter().any(|place| place == HELD.handle()),
        "{trail:?}"
    );
}

/// What runs moves with the mode chosen: the mode that stops for a person says where, the mode
/// with no gate drops that row, and choosing back the first brings it again.
#[tokio::test]
async fn what_runs_says_where_the_mode_chosen_stops_and_nothing_where_it_does_not() {
    let root = tempfile::tempdir().unwrap();
    let (state, performer) = on_the_first_request(root.path()).await;
    let mut strokes = typed("greet ana");
    strokes.extend([Stroke::Enter, Stroke::Tab, Stroke::Tab, Stroke::Tab]);
    let (mut state, _) = drive(state, Vec::new(), &performer, &strokes).await;
    let stops = "it stops for you at approve";
    pressed(&mut state, &[Stroke::Right]);
    assert!(drawn(&state).contains(stops), "{}", drawn(&state));
    pressed(&mut state, &[Stroke::Left]);
    let quick = drawn(&state);
    assert!(quick.contains("what runs"), "{quick}");
    assert!(!quick.contains("it stops for you"), "{quick}");
    pressed(&mut state, &[Stroke::Right]);
    assert!(drawn(&state).contains(stops), "{}", drawn(&state));
}

/// A required input left empty is refused by admission as `yunta run` refuses it, said under
/// that input's row, the focus moved there and its legend standing; the request stays as it
/// was typed, nothing is asked again and no note says it.
#[tokio::test]
async fn an_input_left_empty_is_refused_under_its_row() {
    let root = tempfile::tempdir().unwrap();
    let (state, performer) = on_the_first_request(root.path()).await;
    let mut strokes = typed("greet ana");
    strokes.extend([Stroke::Enter, Stroke::Enter]);
    let refused = driven(state, Vec::new(), &performer, &strokes).await;
    let starts = refused.performed.iter();
    let starts = starts.filter(|operation| matches!(operation, Operation::Start { .. }));
    assert_eq!(starts.count(), 1, "{:?}", refused.performed);
    let screen = refused.state.screen(&Markdown.look());
    assert_eq!(screen.noted.map(|noted| noted.said), None);
    let drawn = drawn(&refused.state);
    let refusal = "✗ input `who` is required and has no default; pass `--input who=...`";
    let (described, said) = (drawn.find("whom it greets"), drawn.find(refusal));
    assert!(said.is_some() && described < said, "{drawn}");
    assert!(
        drawn.contains("`type` the who"),
        "the focus on its row: {drawn}"
    );
    assert!(drawn.contains("greet ana"), "{drawn}");
}

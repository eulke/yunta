//! The catalog in the application: the workflows a repository can run, moved
//! through from the field; a workflow checked there shows the page `yunta check`
//! prints, the rule it breaks named; a new workflow is written with the skeleton
//! `yunta new` writes; and the cases run as `yunta test` runs them, each seen as it
//! ends, and stopped when they are let go.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{Stop, WorkflowStem};
use yunta_core::events::Channel;
use yunta_core::WorkflowShape;
use yunta_render::page::Page;
use yunta_render::refusal::Refused;
use yunta_testkit_ui::driving::{carried_out, drive};
use yunta_testkit_ui::sandbox::workspace;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State, WorkflowOperation};

use support::on_the_field;

/// Two nodes that each wait on the other: a cycle `check` refuses.
const CYCLE: &str = "name: cycle\nnodes:\n  - { id: first, kind: bash, run: \"true\", \
                     depends_on: [second] }\n  - { id: second, kind: bash, run: \"true\", \
                     depends_on: [first] }\n";

/// A repository set up for Yunta under `root`, offering one workflow, `cycle`.
fn offering_a_cycle(root: &Path) -> PathBuf {
    let repo = root.join("project");
    std::fs::create_dir_all(repo.join(".yunta/workflows")).expect("its catalog");
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .expect("its config");
    std::fs::write(repo.join(".yunta/workflows/cycle.yaml"), CYCLE).expect("its workflow");
    repo
}

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

#[tokio::test]
async fn a_workflow_checked_in_the_catalog_shows_the_page_check_prints() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = offering_a_cycle(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000000")]).await;
    let strokes = [Stroke::Insert('w'), Stroke::Insert('c')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let shown = main_region(&state);
    let project = workspace(&repo, root.path())
        .project()
        .expect("the project");
    let check = project
        .check(Path::new("cycle"), None, &Stop::new())
        .await
        .expect("the check `yunta check cycle` prints");
    assert!(!check.passes);
    let printed = Markdown.draw(&check.checked.document(&Markdown.look()));
    assert_eq!(shown, printed);
    assert!(
        shown.contains("cycle in depends_on: first -> second -> first"),
        "the rule it breaks is named: {shown}"
    );
    assert_eq!(
        state.screen(&Markdown.look()).trail,
        ["yunta", "workflows", "cycle check"]
    );
}

/// The workflows' reading refused before any is read: what refused it is said, and the
/// workflows are no longer said to be read, as nothing reads them; coming back reads them again.
#[tokio::test]
async fn a_refused_reading_of_the_workflows_is_said_and_no_longer_said_to_read() {
    let (mut state, _) =
        on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000006")]).await;
    let asked = state.update(Input::Stroke(Stroke::Insert('w')));
    let reading = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::ReadCatalog) => Some(*ticket),
        _ => None,
    });
    let reading = reading.unwrap_or_else(|| panic!("the workflows are read: {asked:?}"));
    let refused = Refused::NoRunHere;
    let said = refused.to_string();
    state.update(Input::Outcome(reading, Outcome::Refused(refused)));
    let shown = main_region(&state);
    assert!(shown.contains(&said), "{shown}");
    assert!(
        !shown.contains(yunta_render::screen::READING_THE_WORKFLOWS),
        "{shown}"
    );
    assert_eq!(state.shown_under_way(), None);
}

/// A workflow's check stands through a refusal of the workflows' own reading: the check's
/// page waits on its own work, which lands there, and what refused the reading is said over
/// the workflows.
#[tokio::test]
async fn a_check_stands_through_a_refusal_of_the_workflows_reading() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = offering_a_cycle(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000005")]).await;
    let (mut state, _) = drive(field, Vec::new(), &performer, &[Stroke::Insert('w')]).await;
    let asked_by = |asked: &[Request], picks: fn(&Operation) -> bool| {
        asked.iter().find_map(|request| match request {
            Request::Perform(ticket, work) if picks(work) => Some((*ticket, work.clone())),
            _ => None,
        })
    };
    let checks =
        |work: &Operation| matches!(work, Operation::Workflow(WorkflowOperation::Check { .. }));
    let asked = state.update(Input::Stroke(Stroke::Insert('c')));
    let (checking, check) = asked_by(&asked, checks).expect("the workflow is checked");
    state.update(Input::Stroke(Stroke::Insert('n')));
    let asked = state.update(Input::Stroke(Stroke::Decline));
    let reads = |work: &Operation| *work == Operation::ReadCatalog;
    let (reading, _) = asked_by(&asked, reads).expect("back on the workflows, they are read again");
    let refused = Refused::NoRunHere;
    state.update(Input::Outcome(reading, Outcome::Refused(refused)));
    let checked = carried_out(&performer, checking, check).await.outcome;
    state.update(Input::Outcome(checking, checked));
    let shown = main_region(&state);
    assert!(
        shown.contains("cycle in depends_on: first -> second -> first"),
        "the check's page: {shown}"
    );
}

/// Enter on the workflow pointed at opens a new run with it chosen: the run it asks for is of
/// that workflow.
#[tokio::test]
async fn enter_starts_the_workflow_pointed_at() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = offering_a_cycle(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000001")]).await;
    let strokes = [Stroke::Insert('w'), Stroke::Enter];
    let (mut state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    assert_eq!(state.screen(&Markdown.look()).trail, ["yunta", "new run"]);
    // Every row as the new run opens: `enter` asks for the run at once.
    let asked = state.update(yunta_ui::Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the new run asks for the run: {asked:?}");
    };
    assert_eq!(request.workflow, PathBuf::from("cycle"));
}

/// A repository set up for Yunta under `root`, offering a workflow of one `bash` node
/// running `run` under each of `named`, each with a case that expects it to finish.
fn with_cases(root: &Path, named: &[(&str, &str)]) -> PathBuf {
    let repo = root.join("project");
    std::fs::create_dir_all(repo.join(".yunta/workflows")).expect("its catalog");
    std::fs::create_dir_all(repo.join(".yunta/tests/fixtures")).expect("its cases");
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .expect("its config");
    std::fs::write(
        repo.join(".yunta/tests/fixtures/none.yaml"),
        "dialect: claude-code\nsessions: []\n",
    )
    .expect("a fixture with no sessions");
    for (name, run) in named {
        let workflow =
            format!("name: {name}\nnodes:\n  - {{ id: only, kind: bash, run: \"{run}\" }}\n");
        std::fs::write(repo.join(format!(".yunta/workflows/{name}.yaml")), workflow)
            .expect("its workflow");
        let case = format!(
            "workflow: {name}\nfixture: fixtures/none.yaml\nexpect: {{ final_state: finished }}\n"
        );
        std::fs::write(repo.join(format!(".yunta/tests/{name}.yaml")), case).expect("its case");
    }
    repo
}

/// `words` typed on the line in front, each character a stroke.
fn typed(words: &str) -> Vec<Stroke> {
    words.chars().map(Stroke::Insert).collect()
}

#[tokio::test]
async fn a_new_workflow_is_written_with_the_skeleton_new_writes() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = offering_a_cycle(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000002")]).await;
    let mut strokes = vec![Stroke::Insert('w'), Stroke::Insert('n')];
    strokes.extend(typed("greeter"));
    // The name, then the first shape, `one-node`.
    strokes.extend([Stroke::Enter, Stroke::Enter]);
    let (mut state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    let shown = main_region(&state);
    assert!(shown.contains("greeter.yaml"), "{shown}");
    let elsewhere = tempfile::tempdir().expect("a temporary root");
    let other = offering_a_cycle(elsewhere.path());
    let stem = WorkflowStem::parse("greeter").expect("a stem");
    workspace(&other, elsewhere.path())
        .new_workflow(&stem, WorkflowShape::OneNode, false)
        .await
        .expect("the workflow `yunta new greeter` writes");
    let written = |repo: &Path| {
        std::fs::read_to_string(repo.join(".yunta/workflows/greeter.yaml")).expect("written")
    };
    assert_eq!(written(&repo), written(&other));
    // Back on the workflows, read again with the new one among them.
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::ReadCatalog)] = asked.as_slice() else {
        panic!("the workflows are read again: {asked:?}");
    };
}

#[tokio::test]
async fn the_cases_that_all_end_show_the_tally_test_prints() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_cases(root.path(), &[("quick", "true")]);
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000003")]).await;
    let strokes = [Stroke::Insert('w'), Stroke::Insert('t')];
    let (state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    let shown = main_region(&state);
    assert!(shown.contains("quick"), "{shown}");
    assert!(shown.contains("1 case, 0 failed"), "{shown}");
}

#[tokio::test]
async fn each_case_is_seen_as_it_ends_and_the_rest_stop_when_let_go() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_cases(root.path(), &[("a-quick", "true"), ("b-slow", "sleep 30")]);
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (field, _) = on_the_field(&[&yunta_core::RunId::from("01M7CATALOG000000000000004")]).await;
    let (mut state, _) = drive(field, Vec::new(), &performer, &[Stroke::Insert('w')]).await;
    let asked = state.update(Input::Stroke(Stroke::Insert('t')));
    let [Request::Perform(ticket, Operation::Workflow(WorkflowOperation::RunCases))] =
        asked.as_slice()
    else {
        panic!("the cases are asked to run: {asked:?}");
    };
    let ticket = *ticket;
    let (inputs, mut received) = mpsc::unbounded_channel();
    let cases = Operation::Workflow(WorkflowOperation::RunCases);
    assert!(performer
        .take(Request::Perform(ticket, cases), &inputs)
        .is_none());
    let first_ended = |input: &Input| matches!(input, Input::News(_, News::Case(_)));
    handed_until(&mut state, &mut received, first_ended).await;
    let shown = main_region(&state);
    assert!(shown.contains("a-quick"), "{shown}");
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert_eq!(asked, [Request::LetGo(ticket)]);
    performer.let_go(ticket);
    let ended = |input: &Input| matches!(input, Input::Outcome(..));
    let stopped = handed_until(&mut state, &mut received, ended);
    tokio::time::timeout(Duration::from_secs(20), stopped)
        .await
        .expect("the case under way stops when the cases are let go");
    let shown = main_region(&state);
    assert!(
        shown.contains(yunta_render::screen::CASES_STOPPED),
        "{shown}"
    );
    assert!(!shown.contains("b-slow"), "{shown}");
}

/// What `received` hands back taken by `state`, up to the first input `last` picks, taken too.
async fn handed_until(
    state: &mut State,
    received: &mut mpsc::UnboundedReceiver<Input>,
    last: impl Fn(&Input) -> bool,
) {
    loop {
        let input = received
            .recv()
            .await
            .expect("the work hands back as it runs");
        let ends = last(&input);
        state.update(input);
        if ends {
            return;
        }
    }
}

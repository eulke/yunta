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
use yunta_core::WorkflowShape;
use yunta_render::page::Page;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Performer, Request, State, WorkflowOperation};

use support::{drive, on_the_field, stopped, workspace};

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
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7CATALOG000000000000000",
    ))]);
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
        ["yunta", "workflows", "cycle"]
    );
}

/// Enter on the workflow pointed at opens its start with it picked: the run it asks
/// for is of that workflow.
#[tokio::test]
async fn enter_starts_the_workflow_pointed_at() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = offering_a_cycle(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7CATALOG000000000000001",
    ))]);
    let strokes = [Stroke::Insert('w'), Stroke::Enter];
    let (mut state, _) = drive(field, Vec::new(), &performer, &strokes).await;
    assert_eq!(state.screen(&Markdown.look()).trail, ["yunta", "start"]);
    // A workflow with no inputs and no modes asks its adapter alone.
    let asked = state.update(yunta_ui::Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the start asks for the run: {asked:?}");
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
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7CATALOG000000000000002",
    ))]);
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
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7CATALOG000000000000003",
    ))]);
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
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&yunta_core::RunId::from(
        "01M7CATALOG000000000000004",
    ))]);
    let (mut state, _) = drive(field, Vec::new(), &performer, &[Stroke::Insert('w')]).await;
    let asked = state.update(Input::Stroke(Stroke::Insert('t')));
    let [Request::Perform(ticket, Operation::Workflow(WorkflowOperation::RunCases))] =
        asked.as_slice()
    else {
        panic!("the cases are asked to run: {asked:?}");
    };
    let ticket = *ticket;
    let (inputs, mut received) = mpsc::unbounded_channel();
    let running = {
        let performer = Arc::clone(&performer);
        let cases = Operation::Workflow(WorkflowOperation::RunCases);
        tokio::spawn(async move { performer.perform(ticket, cases, inputs).await })
    };
    loop {
        let input = received
            .recv()
            .await
            .expect("the cases hand back as they run");
        let first_ended = matches!(&input, Input::News(_, News::Case(_)));
        state.update(input);
        if first_ended {
            break;
        }
    }
    assert!(
        main_region(&state).contains("a-quick"),
        "{}",
        main_region(&state)
    );
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert_eq!(asked, [Request::LetGo(ticket)]);
    performer.let_go(ticket);
    tokio::time::timeout(Duration::from_secs(20), running)
        .await
        .expect("the case under way stops when the cases are let go")
        .expect("the cases' task ends");
    let shown = main_region(&state);
    assert!(
        shown.contains(yunta_render::screen::CASES_STOPPED),
        "{shown}"
    );
    assert!(!shown.contains("b-slow"), "{shown}");
}

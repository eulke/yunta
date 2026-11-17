//! The first run: a repository set up for Yunta with no run yet opens on the question
//! of what to build, over its catalog, and what is typed there reaches the start stage,
//! whose choices ask for the run `yunta run` asks for with the same ones. The question
//! stands while the person is on it; coming back to it once a run stands in the field
//! finds the field.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::Markdown;
use yunta_api::RunRequest;
use yunta_core::events::Channel;
use yunta_core::{InputName, RunId};
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Performer, Request, State};

/// A workflow that takes what was asked for as its request.
const GREETS: &str = "name: greet\ninputs:\n  ask: { type: string, request: true }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n";

/// A mock fixture with no sessions: the run started beside the request opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The run started while the first request is typed.
const STARTED: RunId = RunId::from_static("01M7F1RSTREQ0000000STARTED");

/// A repository under `root` set up for Yunta, with one workflow in its catalog and no
/// run.
fn set_up(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    set_up_in(&repo);
    repo
}

/// `repo`, a git repository, set up for Yunta with one workflow in its catalog.
fn set_up_in(repo: &Path) {
    std::fs::create_dir_all(repo.join(".yunta/workflows")).unwrap();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .unwrap();
    std::fs::write(repo.join(".yunta/workflows/greet.yaml"), GREETS).unwrap();
}

/// A set-up repository with no run opens on the first run, over its catalog; what is
/// typed there reaches the start stage as what was asked for.
#[tokio::test]
async fn a_set_up_repository_with_no_runs_opens_here_and_its_words_reach_the_start() {
    let root = tempfile::tempdir().unwrap();
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (opened, asked) = State::opened(now());

    let (state, _) = drive(opened, asked, &performer, &[]).await;
    let first = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(first.contains("What do you want to build?"), "{first}");
    assert!(first.contains("greet"), "the catalog under it: {first}");

    let typed: Vec<Stroke> = "greet ana"
        .chars()
        .map(Stroke::Insert)
        .chain([Stroke::Enter])
        .collect();
    let (state, _) = drive(state, Vec::new(), &performer, &typed).await;
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some("start"));
    let start = Markdown.screen(&screen);
    assert!(start.contains("asked: greet ana"), "{start}");
}

/// A run started in the store while the request is typed leaves the request standing,
/// what was typed in it; coming back to it from the start finds the field in its place,
/// from what is followed, with no work asked.
#[tokio::test]
async fn a_run_started_while_the_request_is_typed_is_found_coming_back_from_the_start() {
    let world = Bench::with_run_id(STARTED.as_str());
    set_up_in(&world.worktree);
    let root = world
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&world.worktree, root),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    let typed: Vec<Stroke> = "greet ana".chars().map(Stroke::Insert).collect();
    let (state, _) = drive(opened, asked, &performer, &typed).await;

    world.run(FINISHES, NO_SESSIONS).await;
    let (mut state, _) = drive(state, Vec::new(), &performer, &[]).await;
    let kept = state.followed_runs().field.as_ref();
    assert!(
        kept.is_some_and(|field| field.runs.contains_key(&STARTED)),
        "the run started stands in the field followed: {kept:?}"
    );
    let asking = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(asking.contains("What do you want to build?"), "{asking}");
    assert!(
        asking.contains("greet ana"),
        "what was typed stands: {asking}"
    );

    state.update(Input::Stroke(Stroke::Enter));
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some("start"));
    let asked = state.update(Input::Stroke(Stroke::Decline));
    let performs = |request: &Request| matches!(request, Request::Perform(..));
    assert!(
        !asked.iter().any(performs),
        "coming back asks no work: {asked:?}"
    );
    let field = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(field.contains("runs of this repository"), "{field}");
    assert!(
        field.contains("1 closed"),
        "the run started, finished: {field}"
    );
}

/// The workflow and the adapter each list opens on, with what was asked for, ask for
/// the run `yunta run greet --input ask='greet ana'` asks for.
#[tokio::test]
async fn the_choices_ask_for_the_run_yunta_run_asks_for_with_them() {
    let root = tempfile::tempdir().unwrap();
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (opened, asked) = State::opened(now());
    let (mut state, _) = drive(opened, asked, &performer, &[]).await;
    let typed: Vec<Stroke> = "greet ana"
        .chars()
        .map(Stroke::Insert)
        .chain([Stroke::Enter])
        .collect();
    for stroke in typed {
        state.update(Input::Stroke(stroke));
    }

    state.update(Input::Stroke(Stroke::Enter));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::Start { request })] = asked.as_slice() else {
        panic!("the run is asked for: {asked:?}");
    };
    let yunta_run = RunRequest {
        workflow: PathBuf::from("greet"),
        inputs: HashMap::from([(InputName::from("ask"), "greet ana".to_string())]),
        mode: None,
        adapter: None,
        hermetic: false,
    };
    assert_eq!(request, &yunta_run);
}

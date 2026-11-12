//! The first run: a repository set up for Yunta with no run yet opens on the question
//! of what to build, over its catalog, and what is typed there reaches the start stage,
//! whose choices ask for the run `yunta run` asks for with the same ones.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::Markdown;
use yunta_api::RunRequest;
use yunta_core::InputName;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Performer, Request, State};

/// A workflow that takes what was asked for as its request.
const GREETS: &str = "name: greet\ninputs:\n  ask: { type: string, request: true }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n";

/// A repository under `root` set up for Yunta, with one workflow in its catalog and no
/// run.
fn set_up(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join(".yunta/workflows")).unwrap();
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .unwrap();
    std::fs::write(repo.join(".yunta/workflows/greet.yaml"), GREETS).unwrap();
    repo
}

/// A set-up repository with no run opens on the first run, over its catalog; what is
/// typed there reaches the start stage as what was asked for.
#[tokio::test]
async fn a_set_up_repository_with_no_runs_opens_here_and_its_words_reach_the_start() {
    let root = tempfile::tempdir().unwrap();
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
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

/// The workflow and the adapter each list opens on, with what was asked for, ask for
/// the run `yunta run greet --input ask='greet ana'` asks for.
#[tokio::test]
async fn the_choices_ask_for_the_run_yunta_run_asks_for_with_them() {
    let root = tempfile::tempdir().unwrap();
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
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

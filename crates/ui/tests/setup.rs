//! Setting up, in the application, a repository Yunta was never set up in: the answers
//! a person gives become the setup `init -i` writes from the same ones, Escape through
//! every question, a runner's adapter and model among them, writes what `init` writes
//! unasked, and a setup left before it is written writes nothing and lets the project
//! it detected go.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{ProjectDraft, ProjectSetup, ProposedRunners, Repository, RunnerDeclaration};
use yunta_core::{AdapterId, Detected, ModelName, ProbedAdapter, RunnerCandidate, RunnerName};
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{claude_answering, now, workspace, workspace_on};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State, Ticket};

/// What setting a repository up found: the name `project` and the base branch `main`,
/// Claude Code answering and Codex not, and two runners proposed.
fn draft() -> ProjectDraft {
    ProjectDraft {
        repo: PathBuf::from("/home/me/project"),
        name: "project".to_string(),
        base_branch: "main".to_string(),
        detected: Detected::default(),
        probed: vec![
            ProbedAdapter {
                id: AdapterId::from_static("claude-code"),
                healthy: true,
                detail: "2.1.0".to_string(),
            },
            ProbedAdapter {
                id: AdapterId::from_static("codex"),
                healthy: false,
                detail: "not on PATH".to_string(),
            },
        ],
        proposed: ProposedRunners {
            roles: vec![RunnerName::from("executor"), RunnerName::from("reviewer")],
            needs_default: false,
        },
    }
}

/// The application opened in a repository never set up, and the ticket the work that
/// finds what it runs went under.
fn on_the_setup() -> (State, Ticket) {
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, Operation::ReadRepository)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let never_set_up = Outcome::Repository(Repository::NeverSetUp);
    let asked = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(detecting, Operation::DetectProject)] = asked.as_slice() else {
        panic!("a repository never set up opens on finding what it runs: {asked:?}");
    };
    (state, *detecting)
}

/// `words` typed on the line in front, then Enter; what the state asked for.
fn typed(state: &mut State, words: &str) -> Vec<Request> {
    let mut asked = Vec::new();
    for character in words.chars() {
        asked.extend(state.update(Input::Stroke(Stroke::Insert(character))));
    }
    asked.extend(state.update(Input::Stroke(Stroke::Enter)));
    asked
}

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// What `operation` comes to under `ticket`, performed by `performer`.
async fn outcome_of(performer: &Performer, ticket: Ticket, operation: Operation) -> Outcome {
    let (inputs, mut received) = mpsc::unbounded_channel();
    performer.perform(ticket, operation, inputs).await;
    loop {
        match received.recv().await {
            Some(Input::Outcome(_, outcome)) => return outcome,
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
    }
}

/// A git repository named `project` under `root`, never set up.
fn never_set_up(root: &Path) -> PathBuf {
    let repo = root.join("project");
    std::fs::create_dir_all(&repo).expect("a repository's directory");
    yunta_testkit::init_repo(&repo);
    repo
}

#[test]
fn the_answers_make_the_setup_init_writes() {
    let (mut state, detecting) = on_the_setup();
    let found = Outcome::Detected {
        draft: Box::new(draft()),
        held: detecting,
    };
    assert!(state.update(Input::Outcome(detecting, found)).is_empty());
    assert!(main_region(&state).contains("project name"));
    assert!(typed(&mut state, "greeter").is_empty());
    // An empty line keeps the base branch detected.
    assert!(typed(&mut state, "").is_empty());
    // The executor runs on Claude Code, the one adapter that answers, and a model
    // named with a space is refused and asked again.
    assert!(state.update(Input::Stroke(Stroke::Enter)).is_empty());
    assert!(typed(&mut state, "not a model").is_empty());
    assert!(
        main_region(&state).contains("model name"),
        "{}",
        main_region(&state)
    );
    assert!(typed(&mut state, "opus").is_empty());
    // Escape takes the first adapter for the reviewer, and an empty model leaves it to
    // declare later.
    assert!(state.update(Input::Stroke(Stroke::Decline)).is_empty());
    let asked = typed(&mut state, "");
    let [Request::Perform(_, Operation::WriteSetup { held, setup })] = asked.as_slice() else {
        panic!("the last answer writes the setup: {asked:?}");
    };
    assert_eq!(*held, detecting);
    let executor = RunnerCandidate {
        adapter: AdapterId::from_static("claude-code"),
        model: "opus".parse::<ModelName>().expect("a model name"),
        agent: None,
        effort: None,
    };
    let expected = ProjectSetup {
        name: "greeter".to_string(),
        base_branch: "main".to_string(),
        runners: RunnerDeclaration::Declared(BTreeMap::from([(
            RunnerName::from("executor"),
            executor,
        )])),
    };
    assert_eq!(*setup, expected);
}

#[tokio::test]
async fn escape_through_every_question_writes_what_init_writes_unasked() {
    let answered = tempfile::tempdir().expect("a temporary root");
    let repo = never_set_up(answered.path());
    let clis = vec![claude_answering(answered.path())];
    let performer = Arc::new(Performer::new(workspace_on(&repo, answered.path(), clis)));
    let (state, asked) = State::opened(now());
    // As many as any project proposes runners for: once it is written, Escape does
    // nothing more.
    let escapes = [Stroke::Decline; 24];
    let (mut state, surfaced) = drive(state, asked, &performer, &escapes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    assert!(
        main_region(&state).contains("for this repo's CLAUDE.md"),
        "{}",
        main_region(&state)
    );
    let unasked = tempfile::tempdir().expect("a temporary root");
    let other = never_set_up(unasked.path());
    let clis = vec![claude_answering(unasked.path())];
    let detected = workspace_on(&other, unasked.path(), clis)
        .detect_project(false, &yunta_api::Stop::new())
        .await
        .expect("what setting it up finds");
    let setup = detected.draft.answered("", "", Vec::new());
    detected.write(setup).await.expect("the setup written");
    let config = |repo: &Path| {
        std::fs::read_to_string(repo.join(".yunta/config.yaml")).expect("a config written")
    };
    assert!(
        config(&repo).contains("# detected: claude-code (2.1.0 (Claude Code))"),
        "the CLI found is the one on the workspace's PATH:\n{}",
        config(&repo)
    );
    assert_eq!(config(&repo), config(&other));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(_, Operation::ReadCatalog)] = asked.as_slice() else {
        panic!("Enter on the setup written goes on to the first run: {asked:?}");
    };
}

#[tokio::test]
async fn a_setup_left_before_it_is_written_writes_nothing() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = never_set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, reading)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let repository = outcome_of(&performer, *opening, reading.clone()).await;
    let asked = state.update(Input::Outcome(*opening, repository));
    let [Request::Perform(detecting, Operation::DetectProject)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let found = outcome_of(&performer, *detecting, Operation::DetectProject).await;
    let Outcome::Detected { draft, .. } = &found else {
        panic!("{found:?}");
    };
    let setup = draft.answered("", "", Vec::new());
    assert!(state.update(Input::Outcome(*detecting, found)).is_empty());
    let asked = state.update(Input::Stroke(Stroke::Interrupt));
    assert_eq!(asked, [Request::LetGo(*detecting), Request::Leave]);
    let (inputs, _received) = mpsc::unbounded_channel();
    for request in asked {
        performer.take(request, &inputs);
    }
    // What was detected was let go: writing it after fails, and writes nothing.
    let writing = Operation::WriteSetup {
        held: *detecting,
        setup,
    };
    let Outcome::Failed(said) = outcome_of(&performer, *opening, writing).await else {
        panic!("a setup whose project was let go is not written");
    };
    assert_eq!(said, yunta_render::screen::SETUP_LET_GO);
    assert!(!repo.join(".yunta/config.yaml").exists());
}

//! The first run: a repository set up for Yunta with no run yet opens on the question
//! of what to build, over its catalog, and what is typed there reaches a new run, in its
//! request's row. The question and what is typed stand while the person is on them, a run
//! started elsewhere counted in the tally; coming back to it once a run stands in the field
//! finds the field. `esc` goes to the field whatever it holds, where the workflows and a new
//! run open.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::components::{Answer, Tally};
use surco::surface::Markdown;
use surco::words::Standing;
use yunta_core::events::Channel;
use yunta_core::RunId;
use yunta_render::page::{FieldGroup, FirstRequest};
use yunta_testkit::workflows::{FINISHES, GATED};
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, Stage, State};

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
/// typed there reaches a new run as its request, in the request's box.
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
    assert_eq!(screen.trail.last().map(String::as_str), Some("new run"));
    let new_run = Markdown.screen(&screen);
    assert!(new_run.contains("> greet ana"), "{new_run}");
}

/// What the legend of `state`'s screen says `key` does, where it names the key.
fn said_of(state: &State, key: &str) -> Option<String> {
    let legend = state.screen(&Markdown.look()).legend;
    let binding = legend
        .0
        .into_iter()
        .find(|binding| binding.keys.named() == key);
    binding.map(|binding| binding.does)
}

/// The first request of a repository whose field holds no run, opened and read.
async fn asked_first() -> (tempfile::TempDir, PathBuf, Arc<Performer>, State) {
    let root = tempfile::tempdir().unwrap();
    let repo = set_up(root.path());
    let performer = Arc::new(Performer::new(workspace(&repo, root.path()), Channel::Tty));
    let (opened, asked) = State::opened(now());
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    assert!(
        matches!(state.stage(), Some(Stage::FirstRun(_))),
        "a repository with no run opens on the first request"
    );
    (root, repo, performer, state)
}

/// `esc` on the first request of a repository whose field holds no run goes to the field,
/// at the bottom of what stands, its trail naming the repository, and leaves nothing: the
/// legend says so before it is pressed.
#[tokio::test]
async fn esc_on_the_first_request_with_no_run_goes_to_the_field() {
    let (_root, repo, performer, state) = asked_first().await;
    assert_eq!(
        said_of(&state, "esc").as_deref(),
        Some("field"),
        "the legend says `esc` goes to the field"
    );

    let (state, surfaced) = drive(state, Vec::new(), &performer, &[Stroke::Decline]).await;
    assert!(
        !surfaced
            .iter()
            .any(|request| matches!(request, Request::Leave)),
        "`esc` leaves nothing: {surfaced:?}"
    );
    assert!(
        matches!(state.stage(), Some(Stage::Field(_))),
        "`esc` stands on the field: {:?}",
        state.stage()
    );
    assert_eq!(state.stages_under().count(), 0, "the field stands alone");
    let trail = state.screen(&Markdown.look()).trail;
    let place = repo.display().to_string();
    assert_eq!(
        trail,
        ["yunta", place.as_str()],
        "the field of the repository"
    );
}

/// From the field `esc` leaves the first request for, with no run in it, `w` opens the
/// workflows over it, and `n`, which its legend names, a new run that asks what the first
/// request asked.
#[tokio::test]
async fn the_field_with_no_run_opens_the_workflows_and_a_new_run() {
    let (_root, _repo, performer, state) = asked_first().await;
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Decline]).await;
    assert!(
        matches!(state.stage(), Some(Stage::Field(_))),
        "`esc` stands on the field: {:?}",
        state.stage()
    );
    assert_eq!(said_of(&state, "n").as_deref(), Some("new run"));

    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('w')]).await;
    assert!(
        matches!(state.stage(), Some(Stage::Catalog(_))),
        "`w` opens the workflows: {:?}",
        state.stage()
    );
    let workflows = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(workflows.contains("greet"), "{workflows}");
    assert!(
        matches!(state.stages_under().next(), Some(Stage::Field(_))),
        "over the field"
    );

    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Decline]).await;
    assert!(matches!(state.stage(), Some(Stage::Field(_))));
    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('n')]).await;
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some("new run"));
    let asked = Markdown.screen(&screen);
    assert!(
        asked.contains("What do you want to build?"),
        "the new run asks what the first request asked: {asked}"
    );
    assert!(
        matches!(state.stages_under().next(), Some(Stage::Field(_))),
        "over the field"
    );
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
    assert_eq!(screen.trail.last().map(String::as_str), Some("new run"));
    let asked = state.update(Input::Stroke(Stroke::Decline));
    let performs = |request: &Request| matches!(request, Request::Perform(..));
    assert!(
        !asked.iter().any(performs),
        "coming back asks no work: {asked:?}"
    );
    let place = world.worktree.display().to_string();
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(
        trail,
        ["yunta", place.as_str()],
        "the field of the repository"
    );
    let kept = state.followed_runs().field.as_ref();
    let started = kept
        .and_then(|field| field.runs.get(&STARTED))
        .map(|run| run.row(now()));
    assert_eq!(started.map(|run| run.group()), Some(FieldGroup::Closed));
}

/// Words typed on the first request stand under a run started elsewhere: news of the run
/// count it in the tally and name it in the attention row, with no key since every key types
/// there, while the question and what was typed stay until the person leaves them.
#[tokio::test]
async fn words_typed_stand_while_a_run_started_elsewhere_shows_in_the_tally() {
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
    let typed: Vec<Stroke> = "abc".chars().map(Stroke::Insert).collect();
    let (state, _) = drive(opened, asked, &performer, &typed).await;

    world.run(GATED, NO_SESSIONS).await;
    let (state, _) = drive(state, Vec::new(), &performer, &[]).await;
    let screen = state.screen(&Markdown.look());
    let drawn = Markdown.screen(&state.screen(&Markdown.look()));
    let counted: &[_] = match &screen.tally {
        Tally::Counted(counts) => counts,
        Tally::Uncounted => &[],
    };
    let counted = counted.iter().map(|count| (count.standing, count.runs));
    assert!(
        counted.eq([(Standing::NeedsYou, 1)]),
        "the tally counts the run: {drawn}"
    );
    let attention = screen.attention.as_ref();
    let named = attention.and_then(|attention| attention.subject.as_deref());
    assert_eq!(named, Some(STARTED.handle()), "{drawn}");
    assert!(
        attention.is_some_and(|attention| matches!(attention.answer, Answer::Unkeyed)),
        "no key answers it where every key types: {drawn}"
    );
    assert!(drawn.contains(FirstRequest::QUESTION), "{drawn}");
    assert!(drawn.contains("abc"), "what was typed stands: {drawn}");
    assert!(state.typing(), "the words are still typed: {drawn}");
}

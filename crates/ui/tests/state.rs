//! The application as values, through `State` alone: what it asks as it opens, an
//! answer that comes late, where it stands outside a repository, in one set up once the
//! field's first news comes, and where it stands opened on a step; and, driven by its
//! performer, what opening runs.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use surco::look::Look;
use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{FieldReach, FollowNews, Followed, Repository};
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::page::{Page, Signpost};
use yunta_testkit::stubs;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace, workspace_on};
use yunta_testkit_ui::screens;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, RepositoryReading, Request, State};

/// How long a following takes at most to hand its field whole: its first look reads it,
/// so only a following that never reads waits thirty seconds.
const FIRST_LOOK: Duration = Duration::from_secs(30);

/// The run of the field the application first stands on.
const STANDS: RunId = RunId::from_static("01M7STATE00000000000STANDS");

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// The repository read as `repository`, its project keeping its runs in a store of its
/// own and pricing nothing.
fn read_as(repository: Repository) -> Outcome {
    Outcome::Repository(Box::new(RepositoryReading {
        repository,
        storage: PathBuf::from("/home/me/.yunta/yunta.db"),
        prices: None,
    }))
}

/// The application opened and told its repository is set up, and what it asks then.
fn opened_set_up() -> (State, Vec<Request>) {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let asked = state.update(Input::Outcome(*ticket, read_as(Repository::SetUp)));
    (state, asked)
}

/// The first news a following of the field here hands in `bench`'s world, as the performer
/// hands it to the state: the field read whole.
async fn the_fields_first_news(bench: &Bench) -> Input {
    let root = bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Arc::new(Performer::new(
        workspace(&bench.worktree, root),
        Channel::Tty,
    ));
    let (inputs, mut received) = mpsc::unbounded_channel();
    let here = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    performer.take(Request::Follow(here), &inputs);
    let first = tokio::time::timeout(FIRST_LOOK, received.recv())
        .await
        .expect("the following reads its field")
        .expect("the following hands what it reads");
    let Input::FollowNews(FollowNews::Field(_)) = &first else {
        panic!("a following hands its field whole first: {first:?}");
    };
    first
}

/// In a repository set up, the repository read asks no reading of the field: the
/// application stands nowhere, the runs said to be read, and follows the field here; the
/// field's first news stands it on the field.
#[tokio::test]
async fn in_a_repository_set_up_the_fields_first_news_stands_it_on_the_field() {
    let bench = Bench::with_run_id(STANDS.as_str());
    bench.run(FINISHES, NO_SESSIONS).await;
    let (mut state, asked) = opened_set_up();
    let here = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    assert_eq!(asked, [Request::Follow(here)], "it follows the field alone");
    assert!(state.waiting_on().is_empty(), "{:?}", state.waiting_on());
    let reading = main_region(&state);
    assert!(
        reading.contains(yunta_render::screen::READING_THE_RUNS),
        "{reading}"
    );

    let asked = state.update(the_fields_first_news(&bench).await);
    assert!(asked.is_empty(), "{asked:?}");
    let field = main_region(&state);
    assert!(field.contains("runs of this repository"), "{field}");
    assert!(
        field.contains("1 closed"),
        "the bench's run, finished: {field}"
    );
}

/// In a repository set up with no run, the field's first news, empty, stands the
/// application on the first request, which reads its catalog.
#[tokio::test]
async fn in_a_repository_with_no_run_the_fields_first_news_stands_it_on_the_first_request() {
    let bench = Bench::new();
    let (mut state, _) = opened_set_up();
    let asked = state.update(the_fields_first_news(&bench).await);
    assert!(
        matches!(
            asked.as_slice(),
            [Request::Perform(_, Operation::ReadCatalog)]
        ),
        "{asked:?}, waiting on {:?}",
        state.waiting_on()
    );
    let asking = main_region(&state);
    assert!(asking.contains("What do you want to build?"), "{asking}");
}

/// Opening, the application asks where it stands before it shows anything.
#[test]
fn opening_asks_where_it_stands() {
    let (state, requests) = State::opened(now());
    let [Request::Perform(_, Operation::ReadRepository)] = requests.as_slice() else {
        panic!("opening asks whether it stands in a repository: {requests:?}");
    };
    assert_eq!(
        main_region(&state),
        "",
        "nothing stands on screen until it is answered"
    );
}

/// Outside a repository the screen is the signpost, and its trail starts at yunta.
#[test]
fn outside_a_repository_the_screen_is_the_signpost() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let asked = state.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    assert!(asked.is_empty(), "{asked:?}");
    let look = Markdown.look();
    assert_eq!(
        main_region(&state),
        Markdown.draw(&Signpost.document(&look))
    );
    assert_eq!(state.screen(&Look::plain()).trail, ["yunta"]);
}

/// An answer to a ticket the state no longer waits on changes nothing: a stage left
/// can never be undone by a late answer.
#[test]
fn a_late_outcome_is_dropped() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    state.update(Input::Outcome(*ticket, read_as(Repository::NeverSetUp)));
    let before = main_region(&state);
    // Taken, this answer would put the signpost on screen.
    let asked = state.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(main_region(&state), before);
}

/// Where the application could not learn where it stands, its screen says what
/// stopped it, and leaving is what is left.
#[test]
fn a_failure_to_learn_where_it_stands_is_said() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let failed = Outcome::Failed("git is not on PATH".to_string());
    state.update(Input::Outcome(*ticket, failed));
    assert!(
        main_region(&state).contains("git is not on PATH"),
        "{}",
        main_region(&state)
    );
    let left = state.update(Input::Stroke(yunta_ui::prompt::Stroke::Insert('q')));
    assert_eq!(left, [Request::Leave]);
}

/// The state is a value: a clone goes its own way.
#[test]
fn a_clone_goes_its_own_way() {
    let (state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let mut answered = state.clone();
    answered.update(Input::Outcome(*ticket, read_as(Repository::Absent)));
    assert_eq!(main_region(&state), "");
    assert_ne!(main_region(&answered), "");
}

/// Opened on `review` of a run waiting on its plan, the application stands at the plan's
/// desk, where a person stands who opened the run from the field and took its `review`
/// (`p`); and it goes back the way they walked: `esc` reaches the run, and `esc` again
/// the field.
#[test]
fn opened_on_review_it_stands_at_the_plan_and_goes_back_the_way_a_person_walks() {
    yunta_testkit::on_a_deep_stack(|| async {
        let look = Markdown.look();
        let drawn = |state: &State| Markdown.screen(&state.screen(&look));
        let to_the_plan = [Stroke::Enter, Stroke::Insert('p')];
        let reviewed = drawn(&screens::reviewed(&[]).await);
        assert!(reviewed.contains("Plan registered. Approve?"), "{reviewed}");
        assert_eq!(reviewed, drawn(&screens::walked_to(&to_the_plan).await));
        let back = drawn(&screens::reviewed(&[Stroke::Decline]).await);
        assert_eq!(back, drawn(&screens::walked_to(&[Stroke::Enter]).await));
        let back_again = drawn(&screens::reviewed(&[Stroke::Decline, Stroke::Decline]).await);
        assert_eq!(back_again, drawn(&screens::field().await));
    });
}

/// Before it first stands somewhere, the application opened on one step shows what it
/// shows opened on another, and waits on the same reading: what tells the two apart is
/// the stages each has yet to open, which a walk keeps each opening by.
#[test]
fn openings_on_two_steps_differ_by_the_stages_they_have_yet_to_open() {
    let opened = |step: NextStep| {
        State::opened_on(now(), &step)
            .expect("a stage takes the step")
            .0
    };
    let (listed, shaped) = (opened(NextStep::List), opened(NextStep::Schemas));
    let look = Markdown.look();
    let drawn = |state: &State| Markdown.screen(&state.screen(&look));
    assert_eq!(drawn(&listed), drawn(&shaped));
    assert_ne!(
        format!("{:?}", listed.stages_to_open()),
        format!("{:?}", shaped.stages_to_open())
    );
}

/// A step that is work, or asks a confirmation first, opens the application nowhere.
#[test]
fn a_step_no_stage_takes_opens_it_nowhere() {
    let run = "01M7THISRUN000000000000000".parse().expect("a run's id");
    assert!(State::opened_on(now(), &NextStep::Cancel { run }).is_none());
}

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// Opening on a repository's runs runs git as the field printed runs it, for the runs
/// that have a branch in the repository: once, the following finding where it stands. Its
/// git directory is read off `.git`, and whether there is a repository needs no branch.
/// Each git costs a process, about 8 ms on a Mac, before anything shows.
#[test]
fn opening_on_the_field_runs_git_as_the_printed_field_does() {
    yunta_testkit::on_a_deep_stack(|| async {
        let bench = Bench::with_run_id("01M7G1T0NCE000000000000000");
        let set_up = bench.worktree.join(".yunta/config.yaml");
        tokio::fs::create_dir_all(set_up.parent().expect("its .yunta"))
            .await
            .expect("its .yunta");
        tokio::fs::write(&set_up, "defaults:\n  isolation: none\n")
            .await
            .expect("its config");
        bench.run(FINISHES, NO_SESSIONS).await;
        let stubbed = tempfile::tempdir().expect("a directory for the git stub");
        let ran = stubbed.path().join("git.log");
        let counting = stubs::git_counting(stubbed.path(), &ran);
        let root = bench
            .store_path()
            .parent()
            .expect("a bench's root")
            .to_path_buf();
        let mut workspace = workspace_on(&bench.worktree, &root, vec![stubbed.path().join("bin")]);
        // The stub's directory leads the workspace's own `PATH`; its other variables are
        // handed on to git.
        workspace.env.subprocess_vars = counting
            .into_iter()
            .filter(|(name, _)| name != "PATH")
            .collect();
        let performer = Arc::new(Performer::new(workspace, Channel::Tty));
        let (opened, asked) = State::opened(now());
        let (state, _) = drive(opened, asked, &performer, &[]).await;
        assert!(main_region(&state).contains("runs of this repository"));
        let log = std::fs::read_to_string(&ran).unwrap_or_default();
        assert_eq!(
            log.lines().count(),
            1,
            "git ran once, for the following finding where it stands:\n{log}"
        );
    });
}

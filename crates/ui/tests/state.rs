//! The application as values, through `State` alone: what it asks as it opens, an
//! answer that comes late, where it stands outside a repository, and where it stands
//! opened on a step; and, driven by its performer, what opening runs.

use std::sync::Arc;

use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_api::Repository;
use yunta_core::step::NextStep;
use yunta_render::page::{Page, Signpost};
use yunta_testkit::stubs;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace_on};
use yunta_testkit_ui::screens;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State};

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
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
    let asked = state.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::Absent),
    ));
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
    state.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::NeverSetUp),
    ));
    let before = main_region(&state);
    // Taken, this answer would put the signpost on screen.
    let asked = state.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::Absent),
    ));
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
    answered.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::Absent),
    ));
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

/// Opening on a repository's runs runs git as the field printed runs it: once, for the
/// runs that have a branch in the repository. Its git directory is read off `.git`, and
/// whether there is a repository needs no branch. Each git costs a process, about 8 ms
/// on a Mac, before anything shows.
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
        let performer = Arc::new(Performer::new(workspace));
        let (opened, asked) = State::opened(now());
        let (state, _) = drive(opened, asked, &performer, &[]).await;
        assert!(main_region(&state).contains("runs of this repository"));
        let log = std::fs::read_to_string(&ran).unwrap_or_default();
        assert_eq!(log.lines().count(), 1, "git ran:\n{log}");
    });
}

//! The application as values, through `State` alone: what it asks as it opens, an answer
//! that comes late, where it stands outside a repository, in one set up once the field's
//! first news comes or a look at it fails first, and where it stands opened on a step; and,
//! driven by its performer, what opening runs, and a run started here on its way in.

#[path = "state/on_its_way.rs"]
mod on_its_way;
#[path = "state/outside.rs"]
mod outside;
mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use surco::components::{Binding, Keys};
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use tokio::sync::mpsc;
use yunta_api::{FieldReach, FollowNews, Followed, FollowedRun, Repository, RunNews};
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::page::FieldGroup;
use yunta_render::screen::Leaving;
use yunta_replay::registry::Engine;
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

/// `state` stands on the field of the repository at `place`, which holds `run`, closed.
fn on_the_field_holding_closed(state: &State, run: &RunId, place: &str) {
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail, ["yunta", place], "it stands on the field");
    let field = state.followed_runs().field.as_ref();
    let held = field.and_then(|field| field.runs.get(run));
    let held = held.unwrap_or_else(|| panic!("`{run}` stands in the field: {field:?}"));
    assert_eq!(
        held.row(now()).group(),
        FieldGroup::Closed,
        "`{run}` closed"
    );
}

/// The repository read as `repository`, its project keeping its runs in a store of its
/// own and pricing nothing.
fn read_as(repository: Repository) -> Outcome {
    Outcome::Repository(Box::new(RepositoryReading {
        repository,
        storage: PathBuf::from("/home/me/.yunta/yunta.db"),
        prices: None,
        home: None,
    }))
}

/// The application opened and told its repository is set up, and what it asks then.
fn opened_set_up() -> (State, Vec<Request>) {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let asked = state.update(Input::Outcome(
        *ticket,
        read_as(Repository::SetUp {
            dir: PathBuf::from("/home/me/repo"),
        }),
    ));
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
    on_the_field_holding_closed(&state, &STANDS, "/home/me/repo");
}

/// In a repository set up, a look at the field that fails before its first news is said
/// under the runs being read, as the field says one under its list, and `q` and `esc` leave
/// while it stands: the application is never left on a screen with no key.
#[test]
fn a_look_that_fails_before_the_fields_first_news_is_said_and_leaving_is_left() {
    for leaving in [Stroke::Insert('q'), Stroke::Decline] {
        let (mut state, _) = opened_set_up();
        let failed = "the store could not be opened: permission denied".to_string();
        let asked = state.update(Input::LookFailed(failed.clone()));
        assert!(asked.is_empty(), "{asked:?}");
        let shown = main_region(&state);
        assert!(
            shown.contains(yunta_render::screen::READING_THE_RUNS) && shown.contains(&failed),
            "{shown}"
        );
        let legend = state.screen(&Look::plain()).legend;
        let glyphs = surco::glyphs::Glyphs::Unicode;
        assert_eq!(
            support::spelled(&legend, glyphs),
            support::spelled(&yunta_render::screen::leaving_legend(), glyphs)
        );
        assert_eq!(state.update(Input::Stroke(leaving)), [Request::Leave]);
    }
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
        read_as(Repository::NeverSetUp {
            dir: PathBuf::from("/home/me/repo"),
        }),
    ));
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

/// Opened on `review` of a run waiting on its plan, the application stands on the run's
/// screen at the plan's desk in place, the desk a person reads who opened the run from the
/// field, its card standing in place, and read the plan it offers (`r`); `esc` puts the plan
/// aside on the run, as `esc` puts the card aside there, and `esc` again reaches the field,
/// which notes that the run waits.
#[test]
fn opened_on_review_it_stands_at_the_plan_and_esc_twice_reaches_the_field() {
    yunta_testkit::on_a_deep_stack(|| async {
        let look = Markdown.look();
        // Each world stands in a directory of its own, which the field's trail names.
        let drawn = |state: &State| {
            let screen = Markdown.screen(&state.screen(&look));
            let repository = state
                .followed_runs()
                .repository
                .as_ref()
                .and_then(Repository::dir);
            let place = repository.map(|place| place.display().to_string());
            place.map_or(screen.clone(), |place| {
                screen.replace(&place, "<repository>")
            })
        };
        let desk = |state: &State| Markdown.draw(&state.screen(&look).main.doc);
        let to_the_plan = [Stroke::Enter, Stroke::Insert('r')];
        let reviewed = screens::reviewed(&[]).await;
        let drawn_reviewed = drawn(&reviewed);
        assert!(
            drawn_reviewed.contains("Plan registered. Approve?"),
            "{drawn_reviewed}"
        );
        assert_eq!(
            desk(&reviewed),
            desk(&screens::walked_to(&to_the_plan).await)
        );
        let aside = drawn(&screens::reviewed(&[Stroke::Decline]).await);
        let card_aside = [Stroke::Enter, Stroke::Decline];
        assert_eq!(aside, drawn(&screens::walked_to(&card_aside).await));
        let left_from_review = drawn(&screens::reviewed(&[Stroke::Decline, Stroke::Decline]).await);
        let left = [Stroke::Enter, Stroke::Decline, Stroke::Decline];
        assert_eq!(left_from_review, drawn(&screens::walked_to(&left).await));
        assert!(
            left_from_review.contains(Leaving::Waits.said()),
            "{left_from_review}"
        );
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
        let place = bench.worktree.display().to_string();
        let trail = state.screen(&Markdown.look()).trail;
        assert_eq!(trail, ["yunta", place.as_str()], "it stands on the field");
        let log = std::fs::read_to_string(&ran).unwrap_or_default();
        assert_eq!(
            log.lines().count(),
            1,
            "git ran once, for the following finding where it stands:\n{log}"
        );
    });
}

/// A workflow that takes what was asked for as its request.
const GREETS: &str = "name: greet\ninputs:\n  ask: { type: string, request: true }\nnodes:\n  \
                      - { id: say, kind: bash, run: \"true\" }\n";

/// The run started from the first request.
const STARTED: RunId = RunId::from_static("01M7STATE0000000000STARTED");

/// What a start from the first request leaves: the state, its run's screen asked open over
/// the first request by the start's result, the world the run was made in, what carries out
/// the state's work there, and what the state asked as the run's screen opened.
struct StartedHere {
    state: State,
    world: Bench,
    performer: Arc<Performer>,
    opening: Vec<Request>,
}

/// A repository with no run, its first request typed and the run it asks for started, the
/// start's result handed to the state as the engine made the run [`STARTED`] in its store.
async fn started_from_the_first_request() -> StartedHere {
    let world = Bench::with_run_id(STARTED.as_str());
    let set_up = world.worktree.join(".yunta");
    std::fs::create_dir_all(set_up.join("workflows")).expect("its .yunta");
    std::fs::write(set_up.join("config.yaml"), "defaults:\n  isolation: none\n")
        .expect("its config");
    std::fs::write(set_up.join("workflows/greet.yaml"), GREETS).expect("its workflow");
    let root = world.store_path().parent().expect("a bench's root");
    let performer = Arc::new(Performer::new(
        workspace(&world.worktree, root),
        Channel::Tty,
    ));
    let (opened, asked) = State::opened(now());
    // The request goes on to its new run, which reads what it needs as it opens; `enter`
    // starts it with every row as it opens.
    let mut typed: Vec<Stroke> = "greet ana".chars().map(Stroke::Insert).collect();
    typed.push(Stroke::Enter);
    let (mut state, _) = drive(opened, asked, &performer, &typed).await;
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let Some(starting) = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Start { .. }) => Some(*ticket),
        _ => None,
    }) else {
        panic!("the run is asked for: {asked:?}");
    };
    // The run the start asked for, made in the store as the engine makes it.
    world.run(FINISHES, NO_SESSIONS).await;
    let started = Outcome::Started { run: STARTED };
    let opening = state.update(Input::Outcome(starting, started));
    StartedHere {
        state,
        world,
        performer,
        opening,
    }
}

/// On the first request, a run started from it and its screen opened over it, `q` leaves
/// the run for the field, which takes the first request's place at the bottom: the field
/// the state keeps, the run started standing in it, under what leaving the run says.
#[test]
fn leaving_a_run_started_from_the_first_request_ends_on_the_field() {
    yunta_testkit::on_a_deep_stack(|| async {
        let started = started_from_the_first_request().await;
        let (state, opening, performer) = (started.state, started.opening, started.performer);
        let (mut state, _) = drive(state, opening, &performer, &[]).await;
        assert_eq!(state.followed().run, Some(STARTED), "its screen opened");

        let asked = state.update(Input::Stroke(Stroke::Insert('q')));
        assert!(!asked.contains(&Request::Leave), "{asked:?}");
        let place = started.world.worktree.display().to_string();
        on_the_field_holding_closed(&state, &STARTED, &place);
        let screen = state.screen(&Markdown.look());
        let noted = screen.noted.map(|noted| noted.said);
        assert_eq!(noted.as_deref(), Some(Leaving::Ended.said()));
        let field = Markdown.screen(&state.screen(&Markdown.look()));
        assert!(!field.contains("What do you want to build?"), "{field}");
    });
}

/// What `state`'s legend says `q` does, and the note `q` leaves on a copy of it.
fn q_on(state: &State) -> (Option<String>, String) {
    let legend = state.screen(&Look::plain()).legend.0;
    let typed = |binding: &&Binding| matches!(&binding.keys, Keys::Typed(typed) if typed == "q");
    let does = legend
        .iter()
        .find(typed)
        .map(|binding| binding.does.clone());
    let mut left = state.clone();
    left.update(Input::Stroke(Stroke::Insert('q')));
    (does, Markdown.screen(&left.screen(&Markdown.look())))
}

/// A run this window handed to a detached engine itself keeps going while its registry still
/// reads unrecorded, since the engine it was handed to is about to write it: its legend and
/// the note `q` leaves say so. Once its engine is read, here dead, that is what decides, and
/// a later unrecorded reading no longer gives it the benefit of the doubt.
#[test]
fn a_run_started_here_keeps_going_until_its_engine_is_read() {
    yunta_testkit::on_a_deep_stack(|| async {
        let StartedHere {
            mut state, world, ..
        } = started_from_the_first_request().await;
        let FollowNews::Whole { run, events } = support::read_whole(&world).await else {
            unreachable!("a run read whole");
        };
        // As the run stands an instant after the start, at work, its registry not written.
        let (id, manifest) = (run.run().clone(), run.manifest().clone());
        let run = FollowedRun::read(id, manifest, &events[..2], Engine::Unrecorded);
        let whole = FollowNews::Whole {
            run: Box::new(run),
            events: events[..2].to_vec(),
        };
        state.update(Input::FollowNews(whole));
        let (does, field) = q_on(&state);
        assert_eq!(does.as_deref(), Some("field, it keeps going"));
        assert!(field.contains(Leaving::KeepsGoing.said()), "{field}");

        for engine in [Engine::Dead, Engine::Unrecorded] {
            let news = RunNews::Engine(engine);
            state.update(Input::FollowNews(FollowNews::Run { run: STARTED, news }));
        }
        let (does, field) = q_on(&state);
        assert_eq!(does.as_deref(), Some("field"));
        assert!(field.contains(Leaving::Waits.said()), "{field}");
    });
}

//! A run, through `State` alone: opened from the field, what is followed of it folded
//! into what stands across it, the feed of its node at work (`feed`) and its chronicle, a
//! node pointed at or not, the acts it offers, what each key does (`keys`), leaving it for
//! the field (`leaving`), and its marks turning as the reader's look asks.

mod support;

#[path = "run_stage/feed.rs"]
mod feed;
#[path = "run_stage/keys.rs"]
mod keys;
#[path = "run_stage/leaving.rs"]
mod leaving;
#[path = "run_stage/said.rs"]
mod said;

use std::time::Duration;

use surco::components::{Binding, Keys};
use surco::doc::Block;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::surface::{Markdown, Surface, Terminal};
use tokio::sync::mpsc;
use yunta_api::{FollowNews, FollowedRun, RunNews};
use yunta_core::events::StoredEvent;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, Pid, RunId};
use yunta_replay::registry::{Engine, Prompt};
use yunta_replay::RunState;
use yunta_testkit::workflows::{PLANNER_HANDS_TWO_TASKS, PLAN_THEN_GATE};
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_testkit_ui::sandbox::now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, Scene, State};

use support::{on_the_field, performer_in};

/// Two nodes that finish, one after the other.
const FINISHES: &str = "name: done\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                        - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// The cells the region beside a run's pane takes on a terminal from 102 columns: a run's
/// nodes, at its left.
const SIDE: usize = 31;

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The run `bench` ran, as what the application follows reads it first, and every event
/// of its log.
async fn followed(bench: &Bench) -> (FollowedRun, Vec<StoredEvent>) {
    let FollowNews::Whole { run, events } = support::read_whole(bench).await else {
        unreachable!("a run read whole");
    };
    (*run, events)
}

/// `run` as its following would read it whole had its log stopped after `events`.
fn read_whole(run: &FollowedRun, events: &[StoredEvent]) -> FollowNews {
    let mut cut = run.clone();
    cut.state = RunState::replay(events);
    FollowNews::Whole {
        run: Box::new(cut),
        events: events.to_vec(),
    }
}

/// What stands across `state`'s screen, the run's headline, ribbon and gist, as Markdown
/// draws it.
fn across(state: &State) -> String {
    let about = state.screen(&Look::plain()).about;
    Markdown.draw(&about.expect("the run across the screen"))
}

/// The run's pane as Markdown draws it: its title, then what it says.
fn pane(state: &State) -> String {
    let main = state.screen(&Look::plain()).main;
    format!("{:?}\n{}", main.title, Markdown.draw(&main.doc))
}

const RUN: RunId = RunId::from_static("01CCCCCCCCCCCCCCCCCCCCCCCC");

/// A run's screen asks no work as it opens, the state following the run, and going back to
/// the runs asks none either: the field stands as the state keeps it.
#[tokio::test]
async fn opening_a_run_and_going_back_to_the_runs_asks_no_work() {
    let (mut state, _) = on_the_field(&[&RUN]).await;
    let performs = |request: &Request| matches!(request, Request::Perform(..));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    assert!(
        !asked.iter().any(performs),
        "opening the run asks no work: {asked:?}"
    );

    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(
        !asked.iter().any(performs),
        "coming back asks no work: {asked:?}"
    );
    assert!(state.waiting_on().is_empty(), "{:?}", state.waiting_on());
    let field = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(field.contains("runs of this repository"), "{field}");
}

/// The run `read` says, opened from the field and followed, as it reads whole had its
/// log stopped after `events`.
async fn followed_from_the_field(read: &FollowedRun, events: &[StoredEvent]) -> State {
    let (mut state, _) = on_the_field(&[&read.run]).await;
    state.update(Input::Stroke(Stroke::Enter));
    state.update(Input::FollowNews(read_whole(read, events)));
    state
}

/// With a node pointed at by the person, the run's next events, handed as what the state
/// follows hands them, move what stands across the screen while the node's pane stands, and
/// once `f` gives the lead back the pane stands as it does on the run read whole: pointing
/// lets nothing go.
#[tokio::test]
async fn with_a_node_pointed_at_the_runs_news_move_its_rows_and_its_chronicle() {
    let bench = Bench::with_run_id("01M7P1CKEDM0VES00000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let at_once = followed_from_the_field(&whole, &events).await;
    let mut state = followed_from_the_field(&whole, &events[..2]).await;
    let before = across(&state);

    state.update(Input::Stroke(Stroke::Insert('j')));
    let came = FollowNews::Run {
        run: whole.run.clone(),
        news: RunNews::Events(events[2..].to_vec()),
    };
    state.update(Input::FollowNews(came));

    let trail = state.screen(&Look::plain()).trail;
    assert_eq!(
        trail.last().map(String::as_str),
        Some("second"),
        "{trail:?}"
    );
    let after = across(&state);
    assert_ne!(after, before, "the rows move with the run's news");
    assert_eq!(after, across(&at_once), "the rows of the run read whole");
    state.update(Input::Stroke(Stroke::Insert('f')));
    assert_eq!(pane(&state), pane(&at_once), "what came, said");
}

/// The word the run stands at in the headline across its screen, the first block of what
/// stands across.
fn headline_said(state: &State) -> String {
    let about = state.screen(&Look::plain()).about;
    let about = about.expect("the run across the screen");
    match about.blocks().first() {
        Some(Block::Headline(headline)) => headline.said.clone(),
        _ => panic!("the run's headline first: {}", Markdown.draw(&about)),
    }
}

/// A run that finishes while its screen is open says finished in the headline across its
/// screen with no stroke, and stands as its receipt.
#[tokio::test]
async fn a_run_finishing_on_its_screen_says_so_with_no_stroke() {
    let bench = Bench::with_run_id("01M7F1N1SHES0NSCREEN000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let mut state = followed_from_the_field(&whole, &events[..2]).await;
    assert!(
        matches!(state.scene(), Some(Scene::Work)),
        "followed before it finished, while its node's work stands"
    );

    let came = FollowNews::Run {
        run: whole.run.clone(),
        news: RunNews::Events(events[2..].to_vec()),
    };
    state.update(Input::FollowNews(came));
    assert_eq!(headline_said(&state), "finished", "with no stroke");
    assert!(matches!(state.scene(), Some(Scene::Receipt)));
}

/// That `state`'s run stands as its receipt, which the trail names, its pointer beside it on
/// `node`.
fn stands_as_its_receipt_pointing_at(state: &State, node: &str) {
    let screen = state.screen(&Look::plain());
    assert_eq!(
        screen.trail.last().map(String::as_str),
        Some(yunta_render::screen::RECEIPT),
        "a run that finished stands as its receipt"
    );
    let side = screen.side.expect("the run's nodes beside its receipt");
    let nodes = Markdown.draw(&side.region.doc);
    let pointed = |row: &str| row.starts_with("* + ") && row.contains(node);
    assert!(nodes.lines().any(pointed), "pointed at {node}: {nodes}");
}

/// Pointing at a node asks nothing and lets nothing go: a node that ended shows its own
/// scene, titled by it and saying how it stands, from what the state follows. On a run that
/// finished, which stands as its receipt, the pointer stands on the last node that ran, and
/// `k` takes it to the node before.
#[tokio::test]
async fn a_node_pointed_at_shows_its_own_scene_and_asks_nothing() {
    let bench = Bench::with_run_id("01M7N0DEP1CKED000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (read, events) = followed(&bench).await;
    let mut state = followed_from_the_field(&read, &events).await;
    stands_as_its_receipt_pointing_at(&state, "second");

    let asked = state.update(Input::Stroke(Stroke::Insert('k')));
    assert!(asked.is_empty(), "{asked:?}");

    let screen = state.screen(&Look::plain());
    assert_eq!(screen.trail.last().map(String::as_str), Some("first"));
    assert_eq!(screen.main.title, ["first", "bash"]);
    let pane = Markdown.draw(&screen.main.doc);
    assert!(
        pane.contains("stands") && pane.contains("finished"),
        "{pane}"
    );
    assert!(!pane.contains("node first of run"), "{pane}");
    let nodes = Markdown.draw(
        &screen
            .side
            .expect("the run's nodes beside its pane")
            .region
            .doc,
    );
    assert!(nodes.contains("first"), "{nodes}");
}

/// A node whose name passes the region beside the run's pane keeps its one row there, cut
/// where it passes, so a pointer moves along the nodes a row at a time.
#[tokio::test]
async fn a_node_named_past_the_side_keeps_its_one_row() {
    let bench = Bench::with_run_id("01M7W1DENAMED0000000000000");
    let named = "name: named\nnodes:\n  - { id: review-at-the-alternate-reviewer, kind: bash, run: \"true\" }\n  \
                 - { id: second, kind: bash, run: \"true\", depends_on: [review-at-the-alternate-reviewer] }\n";
    bench.run(named, NO_SESSIONS).await;
    let (read, events) = followed(&bench).await;
    let state = followed_from_the_field(&read, &events).await;

    let side = state
        .screen(&Look::plain())
        .side
        .expect("the run's nodes beside its pane");
    let beside = Look {
        width: Width::exact(SIDE),
        ..Look::plain()
    };
    let rows: Vec<String> = Terminal::on(beside)
        .pane(&side.region, 10)
        .lines
        .iter()
        .map(|line| line.text().trim_end().to_string())
        .filter(|row| !row.is_empty())
        .collect();
    assert_eq!(rows.len(), 2, "one row to each node: {rows:#?}");
    assert!(
        rows[0].ends_with(beside.glyphs.ellipsis()),
        "the long name cut where it passes: {rows:#?}"
    );
}

/// A run whose engine asks at its own terminal is said to ask there, and offers only
/// stopping it, in the palette: no other act is bound to a key, and stopping asks a
/// confirmation first.
#[tokio::test]
async fn a_run_asking_at_its_terminal_offers_only_stopping() {
    let bench = Bench::with_run_id("01M7ASK1NGATTERM1NA1000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (mut asking, events) = followed(&bench).await;
    asking.engine = Engine::Alive {
        pid: Pid::current(),
        prompt: Some(Prompt {
            node: Some(NodeId::from("first")),
            since: now(),
        }),
        words: None,
    };
    let mut state = followed_from_the_field(&asking, &events[..2]).await;

    let about = state
        .screen(&Look::plain())
        .about
        .expect("what stands across the screen");
    let across = Markdown.draw(&about);
    assert!(across.contains("asking at its terminal"), "{across}");
    let offered = keys::palette_offers(&state);
    let run = asking.run.clone();
    let stopping = yunta_render::command::line(&NextStep::Cancel { run: run.clone() });
    assert!(offered.contains(&stopping), "{offered:?}");
    let others = [
        NextStep::Resume { run: run.clone() },
        NextStep::Close { run: run.clone() },
        NextStep::Receipt { run },
    ];
    for other in others.iter().map(yunta_render::command::line) {
        assert!(!offered.contains(&other), "{offered:?}");
    }
    assert!(state.update(Input::Stroke(Stroke::Insert('r'))).is_empty());
    assert!(state.update(Input::Stroke(Stroke::Insert('x'))).is_empty());
    assert!(state.update(Input::Stroke(Stroke::Insert('c'))).is_empty());
    assert!(
        state.screen(&Look::plain()).overlay.is_some(),
        "stopping asks first"
    );
}

/// How long a following takes at most to hand a bench's run whole and what it waits on:
/// its looks come a tenth of a second apart, so only a state that never catches up waits
/// this long.
const CATCHING_UP: Duration = Duration::from_secs(30);

/// What `key` does on `state`'s run, as the keys under its screen say: none where the
/// legend names no act bound to it.
fn key_does(state: &State, key: &str) -> Option<String> {
    let legend = state.screen(&Look::plain()).legend.0;
    let named = |binding: &Binding| matches!(&binding.keys, Keys::Typed(typed) if typed == key);
    legend.into_iter().find(named).map(|binding| binding.does)
}

/// A run parked on its plan offers what the decision it waits on offers once that is
/// folded from what the state follows, nothing read beside it: before, its work standing
/// still names none of its acts, the decision standing by itself once read; then the plan's
/// desk, whose keys the legend says in place of the run's acts, which the palette lists.
#[tokio::test]
async fn a_run_parked_on_its_plan_offers_what_its_decision_offers_once_it_is_folded() {
    let bench = Bench::with_run_id("01M7PARKED0NP1AN0000000000");
    bench.run(PLAN_THEN_GATE, PLANNER_HANDS_TWO_TASKS).await;
    let (mut state, _) = on_the_field(&[&bench.run_id]).await;
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let follows = |asked: &Request| matches!(asked, Request::Follow(_));
    let follow = asked.into_iter().find(follows);
    let (performer, (inputs, mut received)) = (performer_in(&bench), mpsc::unbounded_channel());
    performer.take(follow.expect("the run is followed"), &inputs);
    let heads = yunta_testkit::heads(bench.store_path());
    let awaited = |news: &RunNews| matches!(news, RunNews::Awaited(Some(_)));
    while !state.caught_up(&heads) {
        let came = tokio::time::timeout(CATCHING_UP, received.recv()).await;
        let Ok(Some(input)) = came else {
            panic!("the state catches up with the run");
        };
        if matches!(&input, Input::FollowNews(FollowNews::Run { news, .. }) if awaited(news)) {
            assert_eq!(
                (key_does(&state, "r"), key_does(&state, "x")),
                (None, None),
                "before it is read"
            );
        }
        state.update(input);
    }
    assert_eq!(key_does(&state, "r"), None);
    assert_eq!(key_does(&state, "x"), None);
    assert_eq!(key_does(&state, "p").as_deref(), Some("proof"));
    let waiting_on = state.waiting_on();
    assert!(
        waiting_on.is_empty(),
        "nothing is read beside: {waiting_on:?}"
    );
}

/// A running run's ribbon plows its running node's stretch on a look that moves, and
/// stands still on a look whose reader asked it to.
#[tokio::test]
async fn the_ribbon_turns_only_on_a_look_that_moves() {
    let bench = Bench::with_run_id("01M7R1BB0NTVRNS00000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (mut running, events) = followed(&bench).await;
    running.engine = Engine::this(Pid::current());
    let state = followed_from_the_field(&running, &events[..2]).await;

    for (motion, plowed) in [(Motion::Moves, true), (Motion::Still, false)] {
        let look = Look {
            motion,
            ..Look::plain()
        };
        let about = state.screen(&look).about.expect("the run's ribbon");
        let ribbon = about.blocks().iter().find_map(|block| match block {
            Block::Ribbon(ribbon) => Some(ribbon),
            _ => None,
        });
        let Some(ribbon) = ribbon else {
            panic!("the ribbon across the screen");
        };
        assert_eq!(ribbon.events.is_some(), plowed, "{motion:?}");
    }
}

/// A moving run's screen says its time to the second, so a surface hands the time in that
/// often: read a [`State::TIME_EVERY`] later, it says a second more.
#[tokio::test]
async fn a_moving_runs_time_reads_a_second_more_each_time_it_is_handed_in() {
    let bench = Bench::with_run_id("01M7T1MEEVERY0000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (mut running, events) = followed(&bench).await;
    running.engine = Engine::this(Pid::current());
    let mut state = followed_from_the_field(&running, &events[..2]).await;
    let across = |state: &State| {
        let about = state
            .screen(&Look::plain())
            .about
            .expect("the run across the screen");
        Markdown.draw(&about)
    };
    let a_while_in = fixed_now() + chrono::Duration::seconds(10);
    state.update(Input::Time(a_while_in));
    assert!(across(&state).contains("10s"), "{}", across(&state));
    let every = chrono::Duration::from_std(State::TIME_EVERY).expect("a cadence in range");
    state.update(Input::Time(a_while_in + every));
    assert!(across(&state).contains("11s"), "{}", across(&state));
}

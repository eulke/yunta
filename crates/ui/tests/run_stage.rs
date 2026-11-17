//! A run, through `State` alone: opened from the field, what is followed of it folded
//! into its rows and its chronicle, a node picked or not, the acts it offers, going back
//! to the runs, and its marks turning as the reader's look asks.

mod support;

#[path = "run_stage/said.rs"]
mod said;

use std::time::Duration;

use surco::components::{Binding, Keys};
use surco::doc::Block;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::{Markdown, Surface, Terminal};
use tokio::sync::mpsc;
use yunta_api::{FollowNews, FollowedRun, RunNews};
use yunta_core::events::StoredEvent;
use yunta_core::{NodeId, Pid, RunId};
use yunta_replay::frame::Effect;
use yunta_replay::registry::{Engine, Prompt};
use yunta_replay::RunState;
use yunta_testkit::workflows::{PLANNER_HANDS_TWO_TASKS, PLAN_THEN_GATE};
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_testkit_ui::sandbox::now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, State};

use support::{on_the_field, performed, performer_in};

/// Two nodes that finish, one after the other.
const FINISHES: &str = "name: done\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                        - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// The cells the region beside a run's page takes on a terminal.
const SIDE: usize = 22;

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

/// What stands across `state`'s screen, the run's ribbon and the rows a person watches
/// it by, as Markdown draws it.
fn across(state: &State) -> String {
    let about = state.screen(&Look::plain()).about;
    Markdown.draw(&about.expect("the run across the screen"))
}

/// The lines of the run's chronicle on the screen, as a pipe writes them.
fn chronicle(state: &State) -> Vec<String> {
    let look = Look::plain();
    let screen = state.screen(&look);
    screen
        .main
        .doc
        .blocks()
        .iter()
        .flat_map(|block| match block {
            Block::Lines(lines) => lines.iter().map(|line| Paint::Plain.line(line)).collect(),
            _ => Vec::new(),
        })
        .filter(|line| line.starts_with('['))
        .collect()
}

/// What a following finds grows the run's chronicle by what came alone: the events that
/// come are said under the ones the run was read whole with, as the whole log says them,
/// and a batch handed twice says nothing again.
#[tokio::test]
async fn news_of_events_grows_the_chronicle() {
    let bench = Bench::with_run_id("01M7RVNSTAGE00000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let at_once = followed_from_the_field(&whole, &events).await;
    let mut state = followed_from_the_field(&whole, &events[..1]).await;

    let before = chronicle(&state);
    let came = FollowNews::Run {
        run: whole.run.clone(),
        news: RunNews::Events(events[1..].to_vec()),
    };
    state.update(Input::FollowNews(came.clone()));
    let after = chronicle(&state);
    state.update(Input::FollowNews(came));
    let again = chronicle(&state);

    assert_eq!(before.len(), 1, "{before:?}");
    assert!(after.starts_with(&before), "{before:?} then {after:?}");
    assert_eq!(after, chronicle(&at_once), "what came, said once");
    assert_eq!(again, after, "a batch handed twice says nothing again");
}

/// The ticket the run's page was asked under, among `asked`.
fn page_read(asked: &[Request]) -> yunta_ui::Ticket {
    asked
        .iter()
        .find_map(|request| match request {
            Request::Perform(ticket, Operation::ReadPage { .. }) => Some(*ticket),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the run's page is read: {asked:?}"))
}

const RUN: RunId = RunId::from_static("01CCCCCCCCCCCCCCCCCCCCCCCC");

/// Going back to the runs lets the run's readings go and asks no work: the field stands
/// as the state keeps it.
#[tokio::test]
async fn going_back_to_the_runs_lets_the_run_go_and_asks_no_work() {
    let (mut state, _) = on_the_field(&[&RUN]).await;
    let page = page_read(&state.update(Input::Stroke(Stroke::Enter)));

    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(asked.contains(&Request::LetGo(page)), "{asked:?}");
    let performs = |request: &Request| matches!(request, Request::Perform(..));
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

/// With a node picked, the run's next events, handed as what the state follows hands
/// them, move the rows across the screen while the node's page stands, and the chronicle
/// under the run's page once it is back: picking lets nothing go.
#[tokio::test]
async fn with_a_node_picked_the_runs_news_move_its_rows_and_its_chronicle() {
    let bench = Bench::with_run_id("01M7P1CKEDM0VES00000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let at_once = followed_from_the_field(&whole, &events).await;
    let mut state = followed_from_the_field(&whole, &events[..2]).await;
    let before = across(&state);

    state.update(Input::Stroke(Stroke::Enter));
    let came = FollowNews::Run {
        run: whole.run.clone(),
        news: RunNews::Events(events[2..].to_vec()),
    };
    state.update(Input::FollowNews(came));

    let trail = state.screen(&Look::plain()).trail;
    assert_eq!(trail.last().map(String::as_str), Some("first"), "{trail:?}");
    let after = across(&state);
    assert_ne!(after, before, "the rows move with the run's news");
    assert_eq!(after, across(&at_once), "the rows of the run read whole");
    state.update(Input::Stroke(Stroke::Decline));
    assert_eq!(chronicle(&state), chronicle(&at_once), "what came, said");
}

/// The word the run's page stands at in its headline, the page's first block.
fn headline_said(state: &State) -> String {
    let screen = state.screen(&Look::plain());
    match screen.main.doc.blocks().first() {
        Some(Block::Headline(headline)) => headline.said.clone(),
        _ => panic!(
            "the page's headline first: {}",
            Markdown.draw(&screen.main.doc)
        ),
    }
}

/// How many times `asked` asks for the run's page.
fn pages_asked(asked: &[Request]) -> usize {
    let page =
        |request: &&Request| matches!(request, Request::Perform(_, Operation::ReadPage { .. }));
    asked.iter().filter(page).count()
}

/// A run that finishes while its screen is open says finished in its headline with no
/// stroke, the page standing at the run as it is kept, and its page is asked once more,
/// once: when the run turns, not again for the same news.
#[tokio::test]
async fn a_run_finishing_on_its_screen_says_so_and_its_page_is_read_once_more() {
    let bench = Bench::with_run_id("01M7F1N1SHES0NSCREEN000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let mut state = followed_from_the_field(&whole, &events[..2]).await;
    let Some((reading, read)) = state
        .waiting_on()
        .into_iter()
        .find(|(_, operation)| matches!(operation, Operation::ReadPage { .. }))
    else {
        panic!("the run's page is asked: {:?}", state.waiting_on());
    };
    let Outcome::Page(mut page) = performed(&bench, read).await.1 else {
        panic!("the run's page is read");
    };
    let mut cut = whole.clone();
    cut.state = RunState::replay(&events[..2]);
    (page.frame, page.state) = (cut.frame(now()), cut.state);
    state.update(Input::Outcome(reading, Outcome::Page(page)));
    assert_ne!(headline_said(&state), "finished", "read before it finished");

    let came = FollowNews::Run {
        run: whole.run.clone(),
        news: RunNews::Events(events[2..].to_vec()),
    };
    let asked = state.update(Input::FollowNews(came.clone()));
    assert_eq!(headline_said(&state), "finished", "with no stroke");
    assert_eq!(pages_asked(&asked), 1, "read once more: {asked:?}");
    let again = state.update(Input::FollowNews(came));
    assert_eq!(pages_asked(&again), 0, "{again:?}");
}

/// Picking a node asks for its page, which stands in place of the run's once it is read,
/// and lets nothing go.
#[tokio::test]
async fn a_node_picked_shows_its_page_and_lets_nothing_go() {
    let bench = Bench::with_run_id("01M7N0DEP1CKED000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (read, events) = followed(&bench).await;
    let mut state = followed_from_the_field(&read, &events).await;

    let asked = state.update(Input::Stroke(Stroke::Enter));
    let let_go = |request: &Request| matches!(request, Request::LetGo(_));
    assert!(!asked.iter().any(let_go), "{asked:?}");
    let Some((reading, node)) = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::ReadNodePage { node, .. }) => {
            Some((*ticket, node.clone()))
        }
        _ => None,
    }) else {
        panic!("the node's page is asked for: {asked:?}");
    };
    assert_eq!(
        node,
        NodeId::from("first"),
        "the pointer starts on the first node"
    );
    let read = Operation::ReadNodePage {
        run: bench.run_id.clone(),
        node,
    };
    let (_, page) = performed(&bench, read).await;
    state.update(Input::Outcome(reading, page));

    let screen = state.screen(&Look::plain());
    assert_eq!(screen.trail.last().map(String::as_str), Some("first"));
    let page = Markdown.draw(&screen.main.doc);
    assert!(page.contains("node first of run"), "{page}");
    let nodes = Markdown.draw(
        &screen
            .side
            .expect("the run's nodes beside its page")
            .region
            .doc,
    );
    assert!(nodes.contains("first"), "{nodes}");
}

/// A node whose name passes the region beside the run's page keeps its one row there, cut
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
        .expect("the run's nodes beside its page");
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
/// stopping it: no other act is bound to a key, and stopping asks a confirmation first.
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
        .expect("the rows the run is watched by, across the screen");
    let across = Markdown.draw(&about);
    assert!(across.contains("asks at its terminal"), "{across}");
    let does: Vec<String> = state
        .screen(&Look::plain())
        .legend
        .0
        .into_iter()
        .map(|binding| binding.does)
        .collect();
    assert!(
        does.iter()
            .any(|said| said == "stops it and everything under it"),
        "{does:?}"
    );
    for offered in [
        "continues it",
        "closes it for good",
        "certifies what it did",
    ] {
        assert!(
            !does.iter().any(|said| said.starts_with(offered)),
            "{does:?}"
        );
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

/// What `p` does on `state`'s run, as the keys under its screen say: none where no act is
/// bound to it.
fn p_does(state: &State) -> Option<String> {
    let legend = state.screen(&Look::plain()).legend.0;
    let p = |binding: &Binding| matches!(&binding.keys, Keys::Typed(key) if key == "p");
    legend.into_iter().find(p).map(|binding| binding.does)
}

/// A run parked on its plan offers reading it once what it waits on is folded from what
/// the state follows, its page never read.
#[tokio::test]
async fn a_run_parked_on_its_plan_offers_reading_it_once_what_it_waits_on_is_folded() {
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
            assert_eq!(p_does(&state), None, "before it is read");
        }
        state.update(input);
    }
    let reads_its_plan = yunta_render::command::effect(Effect::ReadsPlan);
    assert_eq!(p_does(&state).as_deref(), Some(reads_its_plan));
    let page = |(_, work): &(_, Operation)| matches!(work, Operation::ReadPage { .. });
    let unread = state.waiting_on().iter().any(page);
    assert!(unread, "its page is never read");
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
        let Some(Block::Ribbon(ribbon)) = about.blocks().first() else {
            panic!("the ribbon first across the screen");
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

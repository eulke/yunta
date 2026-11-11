//! A run, through `State` alone: opened from the field, what its following hands back
//! folded into its chronicle, following it or not, a node picked, the acts it offers,
//! going back to the runs, and its marks turning as the reader's look asks.

mod support;

use surco::doc::Block;
use surco::look::Look;
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::{Markdown, Surface};
use yunta_api::{FollowedRun, RunNews};
use yunta_core::{NodeId, Pid, RunId};
use yunta_replay::registry::{Engine, Prompt};
use yunta_replay::RunState;
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Request, State};

use support::{now, on_the_field, performed, stopped};

/// Two nodes that finish, one after the other.
const FINISHES: &str = "name: done\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                        - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The run `bench` ran, as its following reads it first.
async fn followed(bench: &Bench) -> FollowedRun {
    let following = Operation::Follow {
        run: bench.run_id.clone(),
    };
    let (found, _) = performed(bench, following).await;
    found
        .into_iter()
        .find_map(|news| match news {
            News::Following(run) => Some(*run),
            News::Run(_) | News::CasesNamed(_) | News::Case(_) => None,
        })
        .expect("the following hands back the run as it reads first")
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

/// What a following finds grows the run's chronicle: the events that come are said
/// under the ones the run was opened with.
#[tokio::test]
async fn news_of_events_grows_the_chronicle() {
    let bench = Bench::with_run_id("01M7RVNSTAGE00000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let whole = followed(&bench).await;
    let mut state = on_the_field(vec![stopped(&whole.run)]);
    let following = following(&state.update(Input::Stroke(Stroke::Enter)));

    let mut opening = whole.clone();
    opening.events.truncate(1);
    opening.state = RunState::replay(&opening.events);
    state.update(Input::News(following, News::Following(Box::new(opening))));
    let before = chronicle(&state);
    let came = RunNews::Events(whole.events[1..].to_vec());
    state.update(Input::News(following, News::Run(came)));
    let after = chronicle(&state);

    assert_eq!(before.len(), 1, "{before:?}");
    assert!(after.len() > before.len(), "{after:?}");
    assert!(after.starts_with(&before), "{before:?} then {after:?}");
}

/// The ticket the run's following was asked under, among `asked`.
fn following(asked: &[Request]) -> yunta_ui::Ticket {
    asked
        .iter()
        .find_map(|request| match request {
            Request::Perform(ticket, Operation::Follow { .. }) => Some(*ticket),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the run is followed: {asked:?}"))
}

const RUN: RunId = RunId::from_static("01CCCCCCCCCCCCCCCCCCCCCCCC");

/// Stopping following lets the following go; following again asks for it anew.
#[test]
fn stopping_following_lets_it_go_and_following_again_asks_anew() {
    let mut state = on_the_field(vec![stopped(&RUN)]);
    let first = following(&state.update(Input::Stroke(Stroke::Enter)));

    let asked = state.update(Input::Stroke(Stroke::Insert('f')));
    assert_eq!(asked, [Request::LetGo(first)]);

    let asked = state.update(Input::Stroke(Stroke::Insert('f')));
    let [Request::Perform(again, Operation::Follow { run })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_ne!(*again, first);
    assert_eq!(run, &RUN);
}

/// Going back to the runs lets the run's readings go and reads the field again.
#[test]
fn going_back_to_the_runs_lets_the_run_go_and_reads_the_field_again() {
    let mut state = on_the_field(vec![stopped(&RUN)]);
    let first = following(&state.update(Input::Stroke(Stroke::Enter)));

    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(asked.contains(&Request::LetGo(first)), "{asked:?}");
    assert!(
        asked
            .iter()
            .any(|request| matches!(request, Request::Perform(_, Operation::Field { .. }))),
        "{asked:?}"
    );
    assert!(
        state
            .waiting_on()
            .iter()
            .all(|(_, work)| matches!(work, Operation::Field { .. })),
        "{:?}",
        state.waiting_on()
    );
}

/// The run stage open on `run` as `read` says it reads: what the state asked for the
/// run's following, and the state.
fn opened_on(read: &FollowedRun) -> (yunta_ui::Ticket, State) {
    let mut state = on_the_field(vec![stopped(&read.run)]);
    let first = following(&state.update(Input::Stroke(Stroke::Enter)));
    state.update(Input::News(first, News::Following(Box::new(read.clone()))));
    (first, state)
}

/// Picking a node stops following and asks for its page, which stands in place of the
/// run's once it is read.
#[tokio::test]
async fn a_node_picked_shows_its_page_and_stops_following() {
    let bench = Bench::with_run_id("01M7N0DEP1CKED000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (first, mut state) = opened_on(&followed(&bench).await);

    let asked = state.update(Input::Stroke(Stroke::Enter));
    assert!(asked.contains(&Request::LetGo(first)), "{asked:?}");
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
    let beside = Markdown.draw(&screen.side.expect("a page beside the run").region.doc);
    assert!(beside.contains("node first of run"), "{beside}");
}

/// A run whose engine asks at its own terminal is said to ask there, and offers only
/// stopping it: no other act is bound to a key, and stopping asks a confirmation first.
#[tokio::test]
async fn a_run_asking_at_its_terminal_offers_only_stopping() {
    let bench = Bench::with_run_id("01M7ASK1NGATTERM1NA1000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let mut asking = followed(&bench).await;
    asking.events.truncate(2);
    asking.state = RunState::replay(&asking.events);
    asking.engine = Engine::Alive {
        pid: Pid::current(),
        prompt: Some(Prompt {
            node: Some(NodeId::from("first")),
            since: now(),
        }),
        words: None,
    };
    let (_, mut state) = opened_on(&asking);

    let main = Markdown.draw(&state.screen(&Look::plain()).main.doc);
    assert!(main.contains("asks at its terminal"), "{main}");
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

/// A running run's ribbon plows its running node's stretch on a look that moves, and
/// stands still on a look whose reader asked it to.
#[tokio::test]
async fn the_ribbon_turns_only_on_a_look_that_moves() {
    let bench = Bench::with_run_id("01M7R1BB0NTVRNS00000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let mut running = followed(&bench).await;
    running.events.truncate(2);
    running.state = RunState::replay(&running.events);
    running.engine = Engine::this(Pid::current());
    let (_, state) = opened_on(&running);

    for (motion, plowed) in [(Motion::Moves, true), (Motion::Still, false)] {
        let look = Look {
            motion,
            ..Look::plain()
        };
        let about = state.screen(&look).about.expect("the run's ribbon");
        let [Block::Ribbon(ribbon)] = about.blocks() else {
            panic!("the ribbon alone across the screen");
        };
        assert_eq!(ribbon.events.is_some(), plowed, "{motion:?}");
    }
}

//! A prompt in place put aside and brought back, through `State` alone, on the design's run
//! where a task asks for a path outside its scope: `esc` puts the card aside, the person
//! pointing at the node it waits at, whose own scene says what it waits on, the attention row
//! naming the run's own decision; `a` brings the card back as the person left it, whatever
//! node they read meanwhile; `esc` again, or `q`, leaves the run for the field, noting that it
//! waits. A prompt belongs to its pause: the pause handed again, and the run followed anew,
//! leave what was typed on it as it was. Before what a parked run asks is read, no prompt
//! stands to bring back, and `a` goes nowhere.

use surco::components::CardBody;
use surco::doc::Block;
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_api::RunNews;
use yunta_render::screen::Leaving;
use yunta_testkit::Bench;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{AwaitedReading, Grounds, Input, Request, Scene};

use super::{card, resting_on};
use crate::support::{on_the_field, read_whole};

/// A gate nobody answers: its run parks on it.
const PARKS: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go, stop] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The node the trail names after the run's handle, where it names one.
fn trail_names(screen: &RunScreen) -> Option<String> {
    let trail = screen.state().screen(&Look::plain()).trail;
    let [_, _, node] = trail.as_slice() else {
        return None;
    };
    Some(node.clone())
}

/// The words typed on the card standing at the foot of `screen`'s main region, before its
/// cursor, where a line is typed on it.
fn typed_on_the_card(screen: &RunScreen) -> Option<String> {
    let main = screen.state().screen(&Look::plain()).main;
    let Some(Block::Card(card)) = main.doc.blocks().last() else {
        return None;
    };
    match &card.body {
        CardBody::Saying { typed, .. } => Some(typed.before.clone()),
        CardBody::Choosing { .. } | CardBody::Round(_) => None,
    }
}

/// What the run's screen asks of the person, handed again as the following hands it for the
/// pause the run stands at: what it read for that pause, unchanged.
fn the_pause_read_again(screen: &RunScreen) -> RunNews {
    let in_view = screen.state().followed_runs().in_view.as_deref();
    let awaited = in_view.and_then(|in_view| in_view.awaited.clone());
    let Some(AwaitedReading::Read { pause, awaited }) = awaited else {
        panic!("the run's decision is read for its pause");
    };
    RunNews::Awaited(Some((pause, awaited)))
}

#[tokio::test]
async fn half_a_note_typed_on_the_card_stays_through_news_of_its_pause() {
    let mut screen = run::opened("scope").await;
    let half = "only for the audit";
    let typing: Vec<Stroke> = half.chars().map(Stroke::Insert).collect();
    screen.pressed(&[Stroke::Insert('n')]).await;
    screen.pressed(&typing).await;
    assert_eq!(typed_on_the_card(&screen).as_deref(), Some(half));
    let again = the_pause_read_again(&screen);
    screen.handed(again);
    assert_eq!(
        typed_on_the_card(&screen).as_deref(),
        Some(half),
        "news of the same pause never build the card again"
    );
    screen.pressed(&[]).await;
    assert_eq!(
        typed_on_the_card(&screen).as_deref(),
        Some(half),
        "the run followed anew leaves the card as it was"
    );
}

#[tokio::test]
async fn esc_puts_the_card_aside_on_its_node_and_a_brings_it_back_as_it_was_left() {
    let mut screen = run::opened("scope").await;
    screen.pressed(&[Stroke::Insert('j')]).await;
    let pointed = |screen: &RunScreen| card(screen).and_then(|card| card.pointed);
    assert_eq!(pointed(&screen).as_deref(), Some("deny"));

    screen.pressed(&[Stroke::Decline]).await;
    assert!(
        matches!(screen.state().scene(), Some(Scene::Node)),
        "the node it waits at stands as its own scene: {:?}",
        screen.state().scene()
    );
    assert_eq!(trail_names(&screen).as_deref(), Some("implement"));
    let drawn = screen.state().screen(&Look::plain());
    let attention = drawn
        .attention
        .expect("the run's own decision is out of sight");
    assert_eq!(attention.subject.as_deref(), Some("Q78GRR"));
    assert!(
        drawn.noted.is_none(),
        "putting a card aside records nothing"
    );

    // A trip to `brief`, and back to the card with `a`.
    screen.pressed(&[Stroke::Insert('k'); 4]).await;
    assert_eq!(trail_names(&screen).as_deref(), Some("brief"));
    screen.pressed(&[Stroke::Insert('a')]).await;
    assert!(
        resting_on(&screen, Grounds::Scope),
        "`a` brings the card back in place: {:?}",
        screen.state().scene()
    );
    assert_eq!(trail_names(&screen).as_deref(), Some("implement"));
    assert_eq!(pointed(&screen).as_deref(), Some("deny"), "as it was left");
}

#[tokio::test]
async fn esc_again_or_q_leaves_the_run_for_the_field_saying_it_waits() {
    for leaving in [Stroke::Decline, Stroke::Insert('q')] {
        let mut screen = run::opened("scope").await;
        screen.pressed(&[Stroke::Decline]).await;
        assert!(matches!(screen.state().scene(), Some(Scene::Node)));
        let mut field = screen.state().clone();
        let asked = field.update(Input::Stroke(leaving));
        assert!(
            !asked.contains(&Request::Leave),
            "{leaving:?} stays in the application: {asked:?}"
        );
        let drawn = field.screen(&Markdown.look());
        assert_eq!(drawn.trail, ["yunta"], "{leaving:?} goes to the field");
        assert_eq!(field.followed().run, None, "no run is followed under it");
        let said = Markdown.draw(&drawn.main.doc);
        assert!(said.contains(Leaving::Waits.said()), "{leaving:?}: {said}");
    }
}

/// Before the following reads what a parked run asks, `a` goes nowhere: the attention row
/// names nothing, no prompt stands to bring back, and what the run asks stands by itself once
/// read.
#[tokio::test]
async fn a_goes_nowhere_before_what_the_run_asks_is_read() {
    let bench = Bench::with_run_id("01M7DEC1DEDVNREAD000000000");
    bench.run(PARKS, NO_SESSIONS).await;
    let run = bench.run_id.clone();
    let read = read_whole(&bench).await;
    let (mut state, _) = on_the_field(&[&run]).await;
    state.update(Input::Stroke(Stroke::Enter));
    state.update(Input::FollowNews(read));
    let trail = state.screen(&Look::plain()).trail;
    let asked = state.update(Input::Stroke(Stroke::Insert('a')));
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(state.screen(&Look::plain()).trail, trail);
}

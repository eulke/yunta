//! The feed of a run's node at work, through `State` alone: what a following finds grows it
//! by what came alone, said as the whole log says it, once.

use surco::doc::Block;
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_api::{FollowNews, RunNews};
use yunta_core::events::StoredEvent;
use yunta_core::{NodeId, Seq};
use yunta_replay::RunState;
use yunta_testkit::Bench;
use yunta_ui::{Input, State};

use super::{followed, followed_from_the_field, FINISHES, NO_SESSIONS};

/// The rows of the feed at the foot of the run's pane, as a terminal says each.
fn feed(state: &State) -> Vec<String> {
    let screen = state.screen(&Look::plain());
    let Some(Block::Feed(feed)) = screen.main.doc.blocks().last() else {
        panic!(
            "the pane ends with a feed: {}",
            Markdown.draw(&screen.main.doc)
        );
    };
    feed.rows.iter().map(|row| row.line.text()).collect()
}

/// How many of `events` the log holds once the node `first` started, and once it ended.
fn while_first_works(events: &[StoredEvent]) -> (usize, usize) {
    let first = NodeId::from("first");
    let held_once = |stood: fn(Option<Seq>, Option<Seq>) -> bool| {
        (1..=events.len())
            .find(|count| {
                let state = RunState::replay(&events[..*count]);
                let node = state.nodes.get(&first);
                node.is_some_and(|node| stood(node.last_started, node.last_terminal))
            })
            .expect("`first` starts and ends")
    };
    let started = held_once(|started, _| started.is_some());
    let ended = held_once(|_, ended| ended.is_some());
    (started, ended)
}

/// What a following finds grows the feed of the node at work by what came alone: the
/// moments that come are said under the ones the run was read whole with, as the whole log
/// says them, and a batch handed twice says nothing again.
#[tokio::test]
async fn news_of_events_grows_the_feed_of_the_node_at_work() {
    let bench = Bench::with_run_id("01M7RVNSTAGE00000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let (started, ended) = while_first_works(&events);
    let at_once = followed_from_the_field(&whole, &events[..ended]).await;
    let mut state = followed_from_the_field(&whole, &events[..started]).await;

    let before = feed(&state);
    let came = FollowNews::Run {
        run: whole.run().clone(),
        news: RunNews::Events(events[started..ended].to_vec()),
    };
    state.update(Input::FollowNews(came.clone()));
    let after = feed(&state);
    state.update(Input::FollowNews(came));
    let again = feed(&state);

    assert_eq!(before.len(), 1, "where it started: {before:?}");
    assert!(after.starts_with(&before), "{before:?} then {after:?}");
    assert!(after.len() > before.len(), "where it ended: {after:?}");
    assert_eq!(after, feed(&at_once), "what came, said once");
    assert_eq!(again, after, "a batch handed twice says nothing again");
}

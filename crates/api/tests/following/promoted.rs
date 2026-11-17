//! A run promoted before it is followed: its successor, already in the log, handed as one
//! that comes, a chain already in the log followed to its end, and none in the log after
//! the time one takes to be created since the close handed at once as none come; and a
//! successor written between two looks handed, whenever the run held read its close.

use std::sync::Arc;

use yunta_api::{FieldReach, FollowNews, Followed, RunNews};
use yunta_core::RunId;
use yunta_testkit_core::{fixed_now, AtClock};

use super::{closed_promoted, next, next_of, whole, World};

/// A following begun on a run whose promotion and successor are already in the log hands
/// the run read whole, then its successor read whole, which it follows from then on.
#[tokio::test]
async fn a_run_promoted_before_it_is_followed_hands_its_successor() {
    let world = World::new();
    let (run, _) = world.born("promoted-before").await;
    world.record(&run, closed_promoted());
    let successor = world.successor_of(&run, "its-successor").await;
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let RunNews::Successor {
        run: handed,
        events,
    } = next_of(&mut following, &mut kept).await
    else {
        panic!("the successor already in the log is handed");
    };
    assert_eq!(handed.run, successor);
    assert_eq!(
        events,
        world.logged(&successor),
        "the successor's log as it stands"
    );
    assert_eq!(kept.run, successor, "the run held is the successor");
    assert!(!following.awaits_successor());
}

/// A chain already in the log is followed to its end, each successor that closed promoted
/// handing its own, though the log lists the last successor before the one it succeeds.
#[tokio::test]
async fn a_chain_promoted_before_it_is_followed_is_followed_to_its_end() {
    let world = World::new();
    let (first, _) = world.born("chain-b").await;
    world.record(&first, closed_promoted());
    let second = world.successor_of(&first, "chain-c").await;
    world.record(&second, closed_promoted());
    let last = world.successor_of(&second, "chain-a").await;
    let mut following = world.follow(&first).await;
    let (mut kept, _) = whole(&mut following).await;

    for successor in [&second, &last] {
        let news = next_of(&mut following, &mut kept).await;
        let RunNews::Successor { run, .. } = news else {
            panic!("each successor is handed in turn, and came {news:?}");
        };
        assert_eq!(run.run, *successor);
    }
    assert!(!following.awaits_successor(), "the chain ends");
}

/// A run that closed promoted longer ago than a successor takes to be created, with none
/// in the log, is handed as having none at once: the wait runs from its close, not from
/// the following.
#[tokio::test]
async fn a_run_promoted_long_ago_with_no_successor_has_none_at_once() {
    let mut world = World::new();
    let (run, _) = world.born("promoted-long-ago").await;
    world.record(&run, closed_promoted());
    let an_hour_on = fixed_now() + chrono::TimeDelta::hours(1);
    world.project.workspace.clock = Arc::new(AtClock(an_hour_on));
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let news = next_of(&mut following, &mut kept).await;
    assert!(matches!(news, RunNews::NoSuccessor), "{news:?}");
    assert!(!following.awaits_successor());
}

/// A run of the field held before it closed, read whole from the field's copy once its
/// promotion and its successor are both written before the next look: the successor is
/// handed, though it stood in the log as the run was read.
#[tokio::test]
async fn a_successor_written_between_two_looks_is_handed() {
    let world = World::new();
    let (first, _) = world.born("between-first").await;
    let (run, _) = world.born("between-promoted").await;
    let field = |run: &RunId| Followed {
        field: Some(FieldReach::Machine),
        run: Some(run.clone()),
    };
    let mut following = world.project.follow(field(&first)).await.unwrap();

    following.refollow(field(&run));
    world.record(&run, closed_promoted());
    let successor = world.successor_of(&run, "between-successor").await;
    loop {
        if let FollowNews::Run {
            news: RunNews::Successor { run: handed, .. },
            ..
        } = next(&mut following).await
        {
            assert_eq!(handed.run, successor);
            break;
        }
    }
}

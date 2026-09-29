//! What a node's record remembers across its attempts.

use yunta_core::events::{
    EventMeta, Failure, NodeEvent, NodeFailedPayload, NodeLedger, NodeStartedPayload, TokenUsage,
};
use yunta_core::{NodeId, TreeId};

/// Folds `events` onto one node, in order.
fn fold(events: Vec<NodeEvent>) -> NodeLedger {
    let node = NodeId::from_static("compare");
    let mut ledger = NodeLedger::default();
    for (seq, event) in events.iter().enumerate() {
        let meta = EventMeta {
            seq: (seq as u64 + 1).into(),
            at: chrono::DateTime::UNIX_EPOCH,
            node: Some(&node),
        };
        ledger.apply(event, &meta);
    }
    ledger
}

fn started(attempt: u32, tree: &str) -> NodeEvent {
    NodeEvent::Started(NodeStartedPayload::attempt_from(
        attempt,
        tree.parse::<TreeId>().unwrap(),
    ))
}

fn failed(outcome: &str) -> NodeEvent {
    NodeEvent::Failed(NodeFailedPayload::new(
        Failure::message(outcome),
        false,
        TokenUsage::default(),
    ))
}

#[test]
fn the_attempt_after_a_failure_remembers_the_tree_that_failure_started_from() {
    let ledger = fold(vec![
        started(1, "a1a1a1a"),
        failed("regression"),
        started(2, "a1a1a1a"),
    ]);
    let record = ledger.get("compare").unwrap();
    let repeated = record.repeats().expect("the same tree, after a failure");
    assert_eq!(repeated.attempt, 1);
    assert_eq!(repeated.failure, Failure::message("regression"));
}

#[test]
fn an_attempt_from_another_tree_repeats_nothing() {
    let ledger = fold(vec![
        started(1, "a1a1a1a"),
        failed("regression"),
        started(2, "b2b2b2b"),
    ]);
    assert_eq!(ledger.get("compare").unwrap().repeats(), None);
}

#[test]
fn an_attempt_that_follows_no_failure_repeats_nothing() {
    let ledger = fold(vec![started(1, "a1a1a1a"), started(2, "a1a1a1a")]);
    assert_eq!(ledger.get("compare").unwrap().repeats(), None);
}

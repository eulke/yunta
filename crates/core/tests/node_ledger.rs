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

/// Folds `events`, each under its node, in order.
fn fold_nodes(events: Vec<(&str, NodeEvent)>) -> NodeLedger {
    let mut ledger = NodeLedger::default();
    for (seq, (node, event)) in events.iter().enumerate() {
        let node = NodeId::from(*node);
        ledger.apply(
            event,
            &EventMeta {
                seq: (seq as u64 + 1).into(),
                at: chrono::DateTime::UNIX_EPOCH,
                node: Some(&node),
            },
        );
    }
    ledger
}

fn finished(tree: &str) -> NodeEvent {
    NodeEvent::Finished(yunta_core::events::NodeFinishedPayload::leaving(
        "ok",
        TokenUsage::default(),
        tree.parse().unwrap(),
    ))
}

fn moved(ledger: &NodeLedger, node: &str) -> bool {
    ledger.get(node).is_some_and(|record| record.tree_moved)
}

#[test]
fn a_close_that_moved_the_tree_marks_the_attempts_beside_it() {
    let ledger = fold_nodes(vec![
        ("lint", started(1, "a1a1a1a")),
        ("fix", started(1, "a1a1a1a")),
        ("fix", finished("b2b2b2b")),
        ("lint", finished("b2b2b2b")),
    ]);
    assert!(
        moved(&ledger, "lint"),
        "lint was open while fix moved the tree"
    );

    let ledger = fold_nodes(vec![
        ("lint", started(1, "a1a1a1a")),
        ("fix", started(1, "a1a1a1a")),
        ("lint", finished("b2b2b2b")),
        (
            "fix",
            NodeEvent::Failed(
                NodeFailedPayload::new(Failure::message("exit 1"), false, TokenUsage::default())
                    .leaving("b2b2b2b".parse().unwrap()),
            ),
        ),
    ]);
    assert!(
        moved(&ledger, "lint"),
        "lint closed after a failing fix started, and the failure moved the tree"
    );
}

#[test]
fn an_attempt_that_started_on_the_tree_the_writer_left_is_not_marked() {
    let ledger = fold_nodes(vec![
        ("fix", started(1, "a1a1a1a")),
        ("lint", started(1, "b2b2b2b")),
        ("fix", finished("b2b2b2b")),
    ]);
    assert!(!moved(&ledger, "lint"));
}

#[test]
fn an_attempt_that_closed_before_the_writer_started_is_not_marked() {
    let ledger = fold_nodes(vec![
        ("lint", started(1, "a1a1a1a")),
        ("lint", finished("a1a1a1a")),
        ("fix", started(1, "a1a1a1a")),
        ("fix", finished("b2b2b2b")),
    ]);
    assert!(!moved(&ledger, "lint"));
}

#[test]
fn a_new_attempt_starts_unmarked() {
    let ledger = fold_nodes(vec![
        ("lint", started(1, "a1a1a1a")),
        ("fix", started(1, "a1a1a1a")),
        ("fix", finished("b2b2b2b")),
        ("lint", finished("b2b2b2b")),
        ("lint", started(2, "b2b2b2b")),
    ]);
    assert!(!moved(&ledger, "lint"));
}

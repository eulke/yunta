//! What the fold from finding events promises: the effective set is the
//! last state of every id nobody withdrew, in the order each was first
//! posted, and a sequence the engine never writes leaves it unmoved.

use proptest::prelude::*;
use yunta_core::events::findings::{FindingLedger, Slot};
use yunta_core::events::{
    EventBody, EventPayload, Finding, FindingPostedPayload, FindingSeverity, FindingUpdatedPayload,
    FindingWithdrawnPayload, StoredEvent,
};
use yunta_core::{FindingId, NodeId, RunId};

fn finding(id: &str, title: &str) -> Finding {
    Finding {
        id: FindingId::try_from(id.to_string()).expect("a well-formed id"),
        severity: FindingSeverity::Major,
        title: title.to_string(),
        location: "src/a.rs:1".to_string(),
        detail: "d".to_string(),
        proposed_criterion: None,
    }
}

fn event(seq: u64, node: &str, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        seq: seq.into(),
        run_id: RunId::from("run-1"),
        node_id: Some(NodeId::from(node)),
        timestamp: chrono::DateTime::UNIX_EPOCH,
        body: EventBody::Known(payload),
    }
}

fn posted(seq: u64, node: &str, id: &str, title: &str) -> StoredEvent {
    event(
        seq,
        node,
        EventPayload::FindingPosted(FindingPostedPayload {
            finding: finding(id, title),
        }),
    )
}

fn updated(seq: u64, node: &str, id: &str, title: &str) -> StoredEvent {
    event(
        seq,
        node,
        EventPayload::FindingUpdated(FindingUpdatedPayload {
            finding: finding(id, title),
        }),
    )
}

fn withdrawn(seq: u64, node: &str, id: &str) -> StoredEvent {
    event(
        seq,
        node,
        EventPayload::FindingWithdrawn(FindingWithdrawnPayload {
            id: FindingId::try_from(id.to_string()).expect("a well-formed id"),
            reason: "no longer stands".to_string(),
        }),
    )
}

#[test]
fn an_update_replaces_the_state_and_keeps_the_place() {
    let log = vec![
        posted(1, "review", "a", "first"),
        posted(2, "review", "b", "second"),
        updated(3, "review", "a", "sharper"),
    ];
    let effective = FindingLedger::of(&log).effective();
    let titles: Vec<&str> = effective.iter().map(|p| p.finding.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["sharper", "second"],
        "an update replaces the content and moves nothing"
    );
}

#[test]
fn a_withdrawal_removes_it_from_the_effective_set() {
    let log = vec![
        posted(1, "review", "a", "first"),
        posted(2, "review", "b", "second"),
        withdrawn(3, "review", "a"),
    ];
    let ledger = FindingLedger::of(&log);
    let ids: Vec<String> = ledger
        .effective()
        .iter()
        .map(|p| p.finding.id.to_string())
        .collect();
    assert_eq!(ids, vec!["b".to_string()]);
    assert!(matches!(
        ledger.status(
            &NodeId::from("review"),
            &FindingId::try_from("a".to_string()).unwrap()
        ),
        Some(Slot::Withdrawn { .. })
    ));
}

#[test]
fn one_node_never_reaches_another_nodes_finding() {
    let log = vec![
        posted(1, "reviewer-a", "dup", "a's own"),
        posted(2, "reviewer-b", "dup", "b's own"),
        // `reviewer-b` withdrawing `dup` touches its own, never `a`'s.
        withdrawn(3, "reviewer-b", "dup"),
    ];
    let effective = FindingLedger::of(&log).effective();
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].node, NodeId::from("reviewer-a"));
    assert_eq!(effective[0].finding.title, "a's own");
}

#[test]
fn a_sequence_the_engine_never_writes_leaves_the_state_unmoved() {
    let unreachable = vec![
        // An update for an id this node never posted.
        updated(1, "review", "ghost", "nothing to replace"),
        // A withdrawal of the same.
        withdrawn(2, "review", "ghost"),
        // A post on an id already withdrawn, and a second post of a live id.
        posted(3, "review", "a", "first"),
        withdrawn(4, "review", "a"),
        posted(5, "review", "a", "revived"),
        posted(6, "review", "b", "second"),
        posted(7, "review", "b", "again"),
    ];
    let ledger = FindingLedger::of(&unreachable);
    let effective = ledger.effective();
    assert_eq!(
        effective.len(),
        1,
        "only `b` stands, with the state of its first post: {effective:?}"
    );
    assert_eq!(effective[0].finding.title, "second");
    assert!(matches!(
        ledger.status(
            &NodeId::from("review"),
            &FindingId::try_from("a".to_string()).unwrap()
        ),
        Some(Slot::Withdrawn { .. }),
    ));
}

/// One step of a log the engine could actually write.
#[derive(Debug, Clone)]
enum Step {
    Post(usize, usize),
    Update(usize, usize),
    Withdraw(usize),
}

proptest! {
    /// Over any log the engine can write — every update and withdrawal
    /// on a live id of its own node, every post on an id never used —
    /// the effective set holds exactly the live ids, each with the
    /// content of its last post or update, in first-post order.
    #[test]
    fn the_fold_is_last_state_per_id_in_first_post_order(
        steps in prop::collection::vec(
            (0usize..4, 0usize..3, 0usize..3),
            0..24,
        )
    ) {
        // Replay the intent against a model, keeping only the steps a
        // run tool would have accepted.
        let mut live: Vec<Option<usize>> = vec![None; 4];   // id -> generation
        let mut gone = [false; 4];
        let mut order: Vec<usize> = Vec::new();
        let mut accepted: Vec<Step> = Vec::new();

        for (id, op, generation) in steps {
            match op {
                0 if live[id].is_none() && !gone[id] => {
                    live[id] = Some(generation);
                    order.push(id);
                    accepted.push(Step::Post(id, generation));
                }
                1 if live[id].is_some() => {
                    live[id] = Some(generation);
                    accepted.push(Step::Update(id, generation));
                }
                2 if live[id].is_some() => {
                    live[id] = None;
                    gone[id] = true;
                    accepted.push(Step::Withdraw(id));
                }
                _ => {}
            }
        }

        let names = ["a", "b", "c", "d"];
        let log: Vec<StoredEvent> = accepted
            .iter()
            .enumerate()
            .map(|(seq, step)| {
                let seq = seq as u64 + 1;
                match step {
                    Step::Post(id, generation) =>
                        posted(seq, "review", names[*id], &generation.to_string()),
                    Step::Update(id, generation) =>
                        updated(seq, "review", names[*id], &generation.to_string()),
                    Step::Withdraw(id) => withdrawn(seq, "review", names[*id]),
                }
            })
            .collect();

        let effective = FindingLedger::of(&log).effective();

        let expected: Vec<(String, String)> = order
            .iter()
            .filter_map(|id| {
                live[*id].map(|generation| (names[*id].to_string(), generation.to_string()))
            })
            .collect();
        let got: Vec<(String, String)> = effective
            .iter()
            .map(|p| (p.finding.id.to_string(), p.finding.title.clone()))
            .collect();

        prop_assert_eq!(got, expected);
    }
}

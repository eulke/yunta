//! What the fold from finding events promises: the effective set is the
//! last state of every id nobody withdrew, in the order each was first
//! posted, and a sequence the engine never writes leaves it unmoved.

use proptest::prelude::*;
use yunta_core::events::findings::{FindingLedger, Slot};
use yunta_core::events::FindingEvent;
use yunta_core::events::{
    EventPayload, Finding, FindingPostedPayload, FindingSeverity, FindingUpdatedPayload,
    FindingWithdrawnPayload,
};
use yunta_core::{FindingId, NodeId};
use yunta_testkit_core::Log;

fn finding(id: &str, title: &str) -> Finding {
    Finding {
        id: FindingId::try_from(id.to_string()).expect("a well-formed id"),
        severity: FindingSeverity::Major,
        title: title.to_string(),
        location: "src/a.rs:1".into(),
        detail: "d".to_string(),
        proposed_criterion: None,
    }
}

fn posted(id: &str, title: &str) -> EventPayload {
    EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
        finding: finding(id, title),
    }))
}

fn updated(id: &str, title: &str) -> EventPayload {
    EventPayload::Findings(FindingEvent::Updated(FindingUpdatedPayload {
        finding: finding(id, title),
    }))
}

fn withdrawn(id: &str) -> EventPayload {
    EventPayload::Findings(FindingEvent::Withdrawn(FindingWithdrawnPayload {
        id: FindingId::try_from(id.to_string()).expect("a well-formed id"),
        reason: "no longer stands".to_string(),
    }))
}

#[test]
fn an_update_replaces_the_state_and_keeps_the_place() {
    let log = Log::for_run("run-1")
        .node("review", posted("a", "first"))
        .node("review", posted("b", "second"))
        .node("review", updated("a", "sharper"))
        .build();
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
    let log = Log::for_run("run-1")
        .node("review", posted("a", "first"))
        .node("review", posted("b", "second"))
        .node("review", withdrawn("a"))
        .build();
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
    let log = Log::for_run("run-1")
        .node("reviewer-a", posted("dup", "a's own"))
        .node("reviewer-b", posted("dup", "b's own"))
        // `reviewer-b` withdrawing `dup` touches its own, never `a`'s.
        .node("reviewer-b", withdrawn("dup"))
        .build();
    let effective = FindingLedger::of(&log).effective();
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].node, Some(NodeId::from("reviewer-a")));
    assert_eq!(effective[0].finding.title, "a's own");
}

#[test]
fn a_sequence_the_engine_never_writes_leaves_the_state_unmoved() {
    let unreachable = Log::for_run("run-1")
        // An update for an id this node never posted.
        .node("review", updated("ghost", "nothing to replace"))
        // A withdrawal of the same.
        .node("review", withdrawn("ghost"))
        // A post on an id already withdrawn, and a second post of a live id.
        .node("review", posted("a", "first"))
        .node("review", withdrawn("a"))
        .node("review", posted("a", "revived"))
        .node("review", posted("b", "second"))
        .node("review", posted("b", "again"))
        .build();
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

#[test]
fn a_finding_the_engine_posts_about_the_run_stands_like_any_other() {
    // The engine reports on the run itself — a cleanup that could not
    // finish, an artifact a distill did not find — and those carry no
    // node. They are counted; what they have no owner for is being
    // updated or withdrawn.
    let log = Log::for_run("run-1")
        .node("review", posted("a", "a node's own"))
        .event(posted("distill-push", "the run could not push"))
        .build();
    let ledger = FindingLedger::of(&log);
    let effective = ledger.effective();
    assert_eq!(effective.len(), 2, "both stand: {effective:?}");
    assert_eq!(effective[1].node, None);
    assert_eq!(
        ledger.effective_of(&NodeId::from("review")).len(),
        1,
        "a node's own artifact holds what that node reported"
    );
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
        let log = accepted
            .iter()
            .fold(Log::for_run("run-1"), |log, step| match step {
                Step::Post(id, generation) =>
                    log.node("review", posted(names[*id], &generation.to_string())),
                Step::Update(id, generation) =>
                    log.node("review", updated(names[*id], &generation.to_string())),
                Step::Withdraw(id) => log.node("review", withdrawn(names[*id])),
            })
            .build();

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

//! What another node answered about a finding stands beside it while the
//! finding does — never in it, and never counted as the finding gone.

use yunta_core::events::findings::{AnswerGiven, FindingLedger};
use yunta_core::events::{
    Finding, FindingAnswer, FindingAnsweredPayload, FindingEvent, FindingPostedPayload,
    FindingSeverity, FindingUpdatedPayload, FindingWithdrawnPayload,
};
use yunta_core::NodeId;

fn finding(id: &str, title: &str) -> Finding {
    Finding {
        id: id.into(),
        severity: FindingSeverity::Blocking,
        title: title.to_string(),
        location: "src/lib.rs:10".into(),
        detail: "detail".to_string(),
        proposed_criterion: None,
    }
}

fn posted(id: &str) -> FindingEvent {
    FindingEvent::Posted(FindingPostedPayload {
        finding: finding(id, "a bug"),
    })
}

fn answered(id: &str, answer: FindingAnswer, why: &str) -> FindingEvent {
    FindingEvent::Answered(FindingAnsweredPayload {
        node: "review".into(),
        id: id.into(),
        answer,
        why: why.to_string(),
    })
}

/// Folds `(node, event)` pairs in order.
fn fold(events: &[(&str, FindingEvent)]) -> FindingLedger {
    let mut ledger = FindingLedger::default();
    for (node, event) in events {
        ledger.apply(Some(&NodeId::from(*node)), event);
    }
    ledger
}

fn answers_to(ledger: &FindingLedger, id: &str) -> Vec<AnswerGiven> {
    ledger
        .answers(Some(&NodeId::from("review")), &id.into())
        .to_vec()
}

fn given(by: &str, answer: FindingAnswer, why: &str) -> AnswerGiven {
    AnswerGiven {
        by: Some(by.into()),
        answer,
        why: why.to_string(),
    }
}

#[test]
fn an_answer_stands_beside_its_finding_and_moves_no_count() {
    let ledger = fold(&[
        ("review", posted("f1")),
        (
            "fix",
            answered("f1", FindingAnswer::Fixed, "the check is in"),
        ),
    ]);

    assert_eq!(
        answers_to(&ledger, "f1"),
        [given("fix", FindingAnswer::Fixed, "the check is in")]
    );
    assert_eq!(ledger.effective().len(), 1, "the finding still stands");
}

#[test]
fn a_later_answer_from_the_same_node_replaces_its_first() {
    let ledger = fold(&[
        ("review", posted("f1")),
        (
            "fix",
            answered("f1", FindingAnswer::Declined, "out of scope"),
        ),
        (
            "fix",
            answered("f1", FindingAnswer::Fixed, "fixed after all"),
        ),
    ]);

    assert_eq!(
        answers_to(&ledger, "f1"),
        [given("fix", FindingAnswer::Fixed, "fixed after all")]
    );
}

#[test]
fn answers_from_two_nodes_stand_side_by_side() {
    let ledger = fold(&[
        ("review", posted("f1")),
        (
            "fix",
            answered("f1", FindingAnswer::Fixed, "the check is in"),
        ),
        (
            "audit",
            answered("f1", FindingAnswer::Declined, "not a bug"),
        ),
    ]);

    assert_eq!(
        answers_to(&ledger, "f1"),
        [
            given("fix", FindingAnswer::Fixed, "the check is in"),
            given("audit", FindingAnswer::Declined, "not a bug"),
        ]
    );
}

#[test]
fn an_update_drops_the_answers_given_to_what_it_said() {
    let updated = FindingEvent::Updated(FindingUpdatedPayload {
        finding: finding("f1", "a worse bug"),
    });
    let withdrawn = FindingEvent::Withdrawn(FindingWithdrawnPayload {
        id: "f1".into(),
        reason: "wrong".to_string(),
    });
    let answering = ("fix", answered("f1", FindingAnswer::Fixed, "done"));

    let after_update = fold(&[
        ("review", posted("f1")),
        answering.clone(),
        ("review", updated),
    ]);
    assert!(answers_to(&after_update, "f1").is_empty());

    let after_withdrawal = fold(&[("review", posted("f1")), answering, ("review", withdrawn)]);
    assert!(answers_to(&after_withdrawal, "f1").is_empty());
}

#[test]
fn an_answer_to_a_finding_that_does_not_stand_is_ignored() {
    let ledger = fold(&[("fix", answered("f9", FindingAnswer::Fixed, "done"))]);

    assert!(answers_to(&ledger, "f9").is_empty());
    assert!(ledger.effective().is_empty());
}

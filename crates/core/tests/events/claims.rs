//! The claim an escalation states: a written summary reads back as written, an
//! engine claim reads back as the object it was written as, and an engine claim
//! is never refused for repeating its record.

use super::*;

#[test]
fn a_plain_string_summary_reads_back_as_a_written_claim() {
    let parsed: Claim = serde_json::from_value(serde_json::json!("Ready to open the PR?"))
        .expect("a string is a summary");
    assert_eq!(parsed, Claim::Written("Ready to open the PR?".to_string()));

    let payload = Escalation::published_to(
        "Ready to open the PR?",
        vec![Fact::labelled("published at", "example/repo#1")].into(),
        "https://example.test/pull/1",
    )
    .expect("the summary states no fact the evidence holds")
    .into_payload();
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["summary"], "Ready to open the PR?", "{json}");
    let read: GateWaitingPayload = serde_json::from_value(json).unwrap();
    assert_eq!(
        read.summary(),
        &Claim::Written("Ready to open the PR?".into())
    );
}

#[test]
fn an_engine_claim_round_trips_as_the_tagged_object_it_is() {
    let claim = Claim::NeedsDecision {
        node: yunta_core::NodeId::from_static("lint"),
    };
    let json = serde_json::to_value(&claim).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"claim": "needs_decision", "node": "lint"})
    );
    let parsed: Claim = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, claim);

    let escalation = Escalation::new(
        Claim::NodeFailed {
            node: yunta_core::NodeId::from_static("lint"),
        },
        Evidence::none(),
        NonEmpty::from((
            GateOption {
                id: "retry".into(),
                label: None,
                tradeoff: None,
                asks: None,
            },
            Vec::new(),
        )),
    )
    .expect("an engine claim repeats no record");
    let payload = escalation.into_payload();
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        json["summary"],
        serde_json::json!({"claim": "node_failed", "node": "lint"}),
        "{json}"
    );
    let read: GateWaitingPayload = serde_json::from_value(json).unwrap();
    assert_eq!(read, payload);
}

#[test]
fn an_engine_claim_is_never_refused_as_repeating_its_record() {
    let menu = NonEmpty::from((
        GateOption {
            id: "retry".into(),
            label: None,
            tradeoff: None,
            asks: None,
        },
        Vec::new(),
    ));
    let escalation = Escalation::new(
        Claim::NodeFailed {
            node: yunta_core::NodeId::from_static("exit"),
        },
        vec![Fact::bare("exit")].into(),
        menu,
    );
    assert!(
        escalation.is_ok(),
        "an engine claim states no fact in words, so it repeats none"
    );
}

#[test]
fn an_awaiting_review_claim_round_trips_as_the_object_with_its_assignee() {
    let claim = Claim::AwaitsReview {
        node: yunta_core::NodeId::from_static("approve"),
        assignee: Some("lead".to_string()),
    };
    let json = serde_json::to_value(&claim).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"claim": "awaits_review", "node": "approve", "assignee": "lead"})
    );
    let parsed: Claim = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, claim);
}

#[test]
fn an_awaiting_review_claim_without_an_assignee_leaves_the_key_out() {
    let claim = Claim::AwaitsReview {
        node: yunta_core::NodeId::from_static("approve"),
        assignee: None,
    };
    let json = serde_json::to_value(&claim).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"claim": "awaits_review", "node": "approve"})
    );
    let parsed: Claim = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, claim);
}

#[test]
fn a_published_gate_keeps_its_awaiting_review_claim_on_the_log() {
    let claim = Claim::AwaitsReview {
        node: yunta_core::NodeId::from_static("approve"),
        assignee: Some("lead".to_string()),
    };
    let payload = Escalation::published_to(
        claim.clone(),
        vec![Fact::labelled("published at", "example/repo#1")].into(),
        "https://example.test/pull/1",
    )
    .expect("an engine claim repeats no record")
    .into_payload();
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        json["summary"],
        serde_json::json!({"claim": "awaits_review", "node": "approve", "assignee": "lead"}),
        "{json}"
    );
    let read: GateWaitingPayload = serde_json::from_value(json).unwrap();
    assert_eq!(read.summary(), &claim);
}

#[test]
fn a_forge_unreachable_claim_round_trips_as_the_object_with_its_assignee() {
    let claim = Claim::ForgeUnreachable {
        node: yunta_core::NodeId::from_static("approve"),
        assignee: "lead".to_string(),
    };
    let json = serde_json::to_value(&claim).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"claim": "forge_unreachable", "node": "approve", "assignee": "lead"})
    );
    let parsed: Claim = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, claim);
}

#[test]
fn a_recheck_claim_round_trips_as_the_object_with_the_reference_it_checks() {
    let claim = Claim::RecheckUnreachable {
        node: yunta_core::NodeId::from_static("approve"),
        external_ref: "https://example.test/pull/1".to_string(),
    };
    let json = serde_json::to_value(&claim).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "claim": "recheck_unreachable",
            "node": "approve",
            "external_ref": "https://example.test/pull/1"
        })
    );
    let parsed: Claim = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, claim);
}

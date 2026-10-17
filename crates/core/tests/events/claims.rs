//! The claim an escalation states: a written summary reads back as written, and
//! an engine claim reads back as the object it was written as.

use super::*;

#[test]
fn a_plain_string_summary_reads_back_as_a_written_claim() {
    let parsed: Claim = serde_json::from_value(serde_json::json!("Ready to open the PR?"))
        .expect("a string is a summary");
    assert_eq!(parsed, Claim::Written("Ready to open the PR?".to_string()));

    let payload = Escalation::published_to(
        Claim::Written("Ready to open the PR?".to_string()),
        PublishedGate {
            url: "https://example.test/pull/1".to_string(),
        },
        "https://example.test/pull/1",
    )
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
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
        }),
        NonEmpty::from((
            GateOption::reserved(yunta_core::reserved::ReservedOption::Retry),
            Vec::new(),
        )),
    );
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
fn an_escalation_writes_its_menu_beside_its_claim_and_its_options_as_data() {
    let payload = Escalation::new(
        Claim::NeedsDecision {
            node: yunta_core::NodeId::from_static("review"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
        }),
        NonEmpty::from((
            GateOption::leading(
                yunta_core::OptionId::from_static("proceed"),
                Leads::On {
                    next: vec![yunta_core::NodeId::from_static("ship")],
                    settles: false,
                },
            ),
            Vec::new(),
        )),
    )
    .into_payload();
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        json["gate"],
        serde_json::json!({"assignee": "lead"}),
        "{json}"
    );
    assert_eq!(
        json["options"],
        serde_json::json!([
            {"id": "proceed", "leads": {"on": {"next": ["ship"], "settles": false}}}
        ]),
        "{json}"
    );
    assert!(json.get("evidence").is_none(), "{json}");
    let read: GateWaitingPayload = serde_json::from_value(json).unwrap();
    assert_eq!(read, payload);
}

#[test]
fn the_budget_loop_scope_and_departure_claims_round_trip_as_their_objects() {
    let task = TaskId::from_static("task-h");
    let claims = [
        (
            Claim::TokensSpent {
                run: RunId::from("run-1"),
            },
            serde_json::json!({"claim": "tokens_spent", "run": "run-1"}),
        ),
        (
            Claim::LoopCapSpent {
                node: yunta_core::NodeId::from_static("implement"),
            },
            serde_json::json!({"claim": "loop_cap_spent", "node": "implement"}),
        ),
        (
            Claim::ScopeAsked {
                task: task.clone(),
                reason: None,
            },
            serde_json::json!({"claim": "scope_asked", "task": "task-h"}),
        ),
        (
            Claim::ScopeAsked {
                task: task.clone(),
                reason: Some("need b.txt".to_string()),
            },
            serde_json::json!({"claim": "scope_asked", "task": "task-h", "reason": "need b.txt"}),
        ),
        (
            Claim::Departs { task },
            serde_json::json!({"claim": "departs", "task": "task-h"}),
        ),
    ];
    for (claim, json) in claims {
        assert_eq!(serde_json::to_value(&claim).unwrap(), json);
        let parsed: Claim = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, claim);
    }
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
        PublishedGate {
            url: "https://example.test/pull/1".to_string(),
        },
        "https://example.test/pull/1",
    )
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

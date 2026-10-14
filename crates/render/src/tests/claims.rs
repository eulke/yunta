//! The words of an escalation's claim: an engine claim is written from its
//! node, the assignee it names and the re-route it closes, and a written
//! claim is kept.

use yunta_core::events::{Claim, ExhaustedReroute, Failure, FailureMenu};
use yunta_core::NodeId;

use crate::claim::sentence;

/// A failure menu whose re-routes to `fix-lint` are used up after `max_reroutes`.
fn exhausted(max_reroutes: u32) -> FailureMenu {
    FailureMenu {
        attempt: max_reroutes + 1,
        failure: Failure::Message {
            outcome: "exit 1".to_string(),
        },
        remedies: Vec::new(),
        stuck: None,
        reroute: Some(ExhaustedReroute {
            goto: NodeId::from("fix-lint"),
            max_reroutes,
            next_mode: None,
        }),
    }
}

#[test]
fn a_gate_that_needs_a_decision_says_so() {
    let claim = Claim::NeedsDecision {
        node: NodeId::from("approve"),
    };
    assert_eq!(sentence(&claim, None), "gate `approve` needs a decision");
}

#[test]
fn a_node_that_failed_says_it_failed() {
    let claim = Claim::NodeFailed {
        node: NodeId::from("lint"),
    };
    assert_eq!(sentence(&claim, None), "node `lint` failed");
}

#[test]
fn a_node_whose_re_routes_are_used_up_names_the_route_it_closes() {
    let claim = Claim::NodeFailed {
        node: NodeId::from("lint"),
    };
    assert_eq!(
        sentence(&claim, Some(&exhausted(1))),
        "node `lint` failed and its 1 re-route to `fix-lint` is exhausted"
    );
    assert_eq!(
        sentence(&claim, Some(&exhausted(2))),
        "node `lint` failed and its 2 re-routes to `fix-lint` are exhausted"
    );
    assert_eq!(
        sentence(&claim, Some(&exhausted(0))),
        "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted"
    );
}

#[test]
fn a_written_claim_reads_as_it_was_written_whatever_the_menu_holds() {
    let claim = Claim::Written("Approve the plan?".to_string());
    assert_eq!(sentence(&claim, None), "Approve the plan?");
    assert_eq!(sentence(&claim, Some(&exhausted(1))), "Approve the plan?");
}

#[test]
fn an_awaiting_review_claim_names_its_node_and_its_assignee() {
    let claim = Claim::AwaitsReview {
        node: NodeId::from("approve"),
        assignee: Some("lead".to_string()),
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve` is waiting on external review (assignee: lead)"
    );
}

#[test]
fn an_awaiting_review_claim_without_an_assignee_names_only_its_node() {
    let claim = Claim::AwaitsReview {
        node: NodeId::from("approve"),
        assignee: None,
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve` is waiting on external review"
    );
}

#[test]
fn a_gate_that_cannot_publish_for_want_of_a_forge_says_so_and_names_its_assignee() {
    let claim = Claim::ForgeUnreachable {
        node: NodeId::from("approve"),
        assignee: "lead".to_string(),
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve` wants an external gate (assignee: lead) but no forge is reachable from this machine (no credentials, or none configured) — resolve it here instead"
    );
}

#[test]
fn a_gate_that_cannot_be_checked_again_names_the_reference_it_was_published_at() {
    let claim = Claim::RecheckUnreachable {
        node: NodeId::from("approve"),
        external_ref: "example/repo#1".to_string(),
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve`'s external gate (published at example/repo#1) needs re-checking, but no forge is reachable from this machine — resolve it here instead"
    );
}

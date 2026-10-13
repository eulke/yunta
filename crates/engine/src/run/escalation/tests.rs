//! The menu a failure's escalation is built with: each reserved option its
//! remedies name is offered once, in the order they first name it, and the
//! menu keeps every remedy it was built from.

use yunta_core::events::{Failure, GateWaitingPayload, Outlook, Precondition, Remedy};
use yunta_core::reserved::ReservedOption;
use yunta_core::NodeId;

use super::build_failure_escalation;

/// A remedy that runs the node again once `precondition` holds.
fn retry(precondition: Precondition) -> Remedy {
    Remedy {
        option: ReservedOption::Retry,
        precondition: Some(precondition),
    }
}

/// The escalation of a node that failed on its second attempt, offering `remedies`.
fn escalation_offering(remedies: Vec<Remedy>) -> GateWaitingPayload {
    build_failure_escalation(
        &NodeId::from("lint"),
        &Failure::message("the linter failed"),
        2,
        &Outlook::Choose(remedies),
    )
    .expect("the claim states no fact the record holds")
    .into_payload()
}

/// The ids of the options a menu offers, in the order it shows them.
fn offered(payload: &GateWaitingPayload) -> Vec<&str> {
    payload
        .options()
        .iter()
        .map(|option| option.id.as_str())
        .collect()
}

#[test]
fn two_remedies_that_name_the_same_option_offer_it_once() {
    let remedies = vec![
        retry(Precondition::CommandRunnable { cmd: "x".into() }),
        retry(Precondition::OutsideTreeChanged),
    ];
    let payload = escalation_offering(remedies.clone());
    assert_eq!(offered(&payload), ["retry", "abort"]);
    let menu = payload
        .failure
        .as_deref()
        .expect("a failure's escalation carries its menu");
    assert_eq!(menu.remedies, remedies);
}

#[test]
fn options_read_in_the_order_the_remedies_first_name_them_with_abort_last() {
    let payload = escalation_offering(vec![
        retry(Precondition::OutsideTreeChanged),
        Remedy {
            option: ReservedOption::ContinueWork,
            precondition: None,
        },
        retry(Precondition::TreeChanged),
    ]);
    assert_eq!(offered(&payload), ["retry", "continue-work", "abort"]);
}

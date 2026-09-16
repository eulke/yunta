//! The capability table: every capability an adapter can declare, and
//! what the engine does when it doesn't.

use yunta_core::port::{absence_of, Absence, POLICY};
use yunta_core::{Capabilities, Capability};

/// A capability with no row is a capability whose absence nobody
/// decided, and a capability with two is two answers to one question.
#[test]
fn every_capability_has_exactly_one_absence_policy() {
    assert_eq!(
        POLICY.len(),
        Capability::ALL.len(),
        "the table names every capability and nothing else"
    );
    for capability in Capability::ALL {
        let rows = POLICY
            .iter()
            .filter(|(named, _)| *named == capability)
            .count();
        assert_eq!(rows, 1, "`{capability}` appears once in the table");
    }
}

/// The lookup answers for every capability, and the answer it gives is
/// the row the table holds.
#[test]
fn the_lookup_answers_with_the_table_for_every_capability() {
    for capability in Capability::ALL {
        let row = POLICY
            .iter()
            .find_map(|(named, absence)| (*named == capability).then_some(absence))
            .expect("the table names every capability");
        assert_eq!(absence_of(capability), row, "`{capability}`");
    }
}

/// `Capabilities::declares` covers the same closed set: a capability
/// that could never be declared is one nothing can ever grant.
#[test]
fn every_capability_is_a_field_an_adapter_can_declare() {
    let none = Capabilities::default();
    for capability in Capability::ALL {
        assert!(
            !none.declares(capability),
            "an adapter declaring nothing declares no `{capability}`"
        );
    }
}

/// Only the two the engine refuses at `check` are refusals: everything
/// else has a stated fallback, so a missing capability never stops a run
/// that already started.
#[test]
fn the_only_capabilities_a_check_refuses_are_the_two_a_workflow_names() {
    let refused: Vec<Capability> = POLICY
        .iter()
        .filter(|(_, absence)| matches!(absence, Absence::FailAtCheck))
        .map(|(capability, _)| *capability)
        .collect();
    assert_eq!(
        refused,
        vec![Capability::PermissionProfiles, Capability::CustomAgents],
        "a workflow that names a permission profile or an agent says so \
         before a run is born; everything else degrades"
    );
}

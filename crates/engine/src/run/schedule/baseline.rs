//! When the run waits for its lineage's measurement.
//!
//! Measured in the run's own tree, the measurement is owed before anything
//! of the run's runs, since anything may change that tree. Measured aside,
//! in a checkout of the commit the run opened on, it is owed only before a
//! step that reads it: the run goes on meanwhile, and the measurement is
//! taken beside it as soon as the run wakes owing it.

use yunta_core::{NodeId, Workflow};

use super::{Decision, Policy};
use crate::replay::RunState;
use crate::run::baseline::waits_for_the_measurement;

/// The measurement this run's lineage declared and its log does not hold.
/// A run born holding one — a `kind: workflow` child, a promotion
/// successor — never owes it.
pub fn owed_baseline<'p>(state: &RunState, policy: &'p Policy) -> Option<&'p str> {
    let suite = policy.baseline_suite.as_deref()?;
    state.run.baseline().is_none().then_some(suite)
}

/// `next`, unless the run owes its measurement before it: always, when it
/// measures in its own tree; only when `next` reads it, when it measures
/// aside.
pub(super) fn before(
    workflow: &Workflow,
    state: &RunState,
    policy: &Policy,
    next: Decision,
) -> Decision {
    match owed_baseline(state, policy) {
        Some(suite) if !policy.measures_aside || reads(workflow, policy, &next) => {
            Decision::MeasureBaseline {
                suite: suite.to_string(),
            }
        }
        _ => next,
    }
}

/// Whether `step` waits for the measurement: it runs a node that does, it
/// asks a person about a gate that shows a plan, or it offers to promote
/// the run, whose successor is born holding the measurement. A loop starts
/// beside it.
fn reads(workflow: &Workflow, policy: &Policy, step: &Decision) -> bool {
    let node = |id: &NodeId| workflow.iter_nodes().find(|node| node.id == *id);
    match step {
        Decision::Execute(batch) => batch
            .iter()
            .filter_map(|(id, _)| node(id))
            .any(waits_for_the_measurement),
        Decision::PublishGate { node: id }
        | Decision::PollGate { node: id, .. }
        | Decision::ResolveInternalGate { node: id } => {
            node(id).is_some_and(waits_for_the_measurement)
        }
        Decision::GateExhaustedReroutes { .. } => policy.may_promote,
        _ => false,
    }
}

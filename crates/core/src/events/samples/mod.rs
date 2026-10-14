//! One sample of every event kind: the payload a test builds when it has
//! to name a kind and does not care what the kind says.
//!
//! The registry names each row's sample by its wire name, so a kind that
//! has no sample does not compile. Each file holds the samples of one
//! domain, in log order.

// A sample is test data: a literal it builds from that does not fit is a
// mistake in the sample, meant to abort the test that uses it.
#![allow(clippy::expect_used)]

mod artifacts;
mod children;
mod findings;
mod gates;
mod node;
mod run;
mod scope;
mod session;
mod tasks;

pub(crate) use artifacts::{artifact_accepted, artifact_submitted, artifact_written};
pub(crate) use children::{child_run_created, child_run_finished, loop_iteration};
pub(crate) use findings::{
    finding_answered, finding_posted, finding_proved, finding_refused, finding_settled,
    finding_updated, finding_withdrawn,
};
pub(crate) use gates::{
    asking_opened, gate_resolved, gate_waiting, questions_answered, questions_asked,
};
pub(crate) use node::{
    context_assembled, criteria_checked, hook_executed, node_failed, node_finished, node_rerouted,
    node_started, pull_request_opened, runner_resolved, scope_checked,
};
pub(crate) use run::{
    baseline_captured, baseline_held, host_suspended, promotion_signaled, run_created,
    run_finished, run_moved, run_paused, run_resumed,
};
pub(crate) use scope::{
    scope_derived, scope_expansion_denied, scope_expansion_granted, scope_expansion_requested,
    scope_expansion_withdrawn,
};
pub(crate) use session::{
    agent_message, agent_session_opened, capability_degraded, rail_failed, rail_summary,
    run_tool_failed, run_tool_refused, service_reachable, service_unreachable, stop_refused,
    write_formatted, write_refused,
};
pub(crate) use tasks::{
    criterion_declared_unpassable, deviation_declared, deviation_resolved, task_check_answered,
    task_check_started, task_registered, task_status_changed,
};

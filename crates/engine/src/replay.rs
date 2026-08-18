//! State derivation by replay (T2.3) — **M-0 cut only**.
//!
//! `derive` is the "functional core" CLAUDE.md asks for: a pure function
//! over an event slice, no IO, safe to call from a property test or from
//! `yunta resume` alike. It tracks what M-0's schema can actually produce
//! today — node lifecycle (`node_started`/`node_finished`/`node_failed`)
//! and task status (`task_registered`/`task_status_changed`) — plus the
//! running token total. Budgets beyond token counting and gates are not
//! derived yet: nothing emits a `gate_waiting`/`gate_resolved` pair until
//! `kind: gate` exists (M5/T7.2), and presupuesto enforcement beyond
//! tokens is T3.3. Extend this the day those events actually appear.
//!
//! A log that is insufficient or inconsistent — e.g. `node_finished` for a
//! node that was never `node_started` — marks the result `broken` with a
//! diagnostic naming the exact event, rather than panicking or guessing
//! (Contrato §8.1). Replay stops at the first such event; the state
//! accumulated up to that point is still returned.

use std::collections::HashMap;

use yunta_core::events::{Event, EventPayload, TaskStatus, TokenUsage};
use yunta_core::{NodeId, TaskId};

/// One node's derived lifecycle state. An enum, not booleans (CLAUDE.md):
/// there is no combination of flags to get wrong.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    Running {
        attempt: u32,
    },
    Finished {
        outcome: String,
        tokens: TokenUsage,
    },
    Failed {
        outcome: String,
        tokens: TokenUsage,
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunState {
    pub nodes: HashMap<NodeId, NodeState>,
    pub tasks: HashMap<TaskId, TaskStatus>,
    pub total_tokens: TokenUsage,
    /// `Some(diagnostic)` once the log has proven insufficient to derive
    /// further state — the point where a `yunta resume`/`status` would
    /// report the run as `broken`.
    pub broken: Option<String>,
}

/// Derives run state from its event log, in `seq` order. Pure: same
/// input, same output, always (T2.3's property test relies on exactly
/// this).
pub fn derive(events: &[Event]) -> RunState {
    let mut state = RunState::default();

    for event in events {
        if let Err(diagnostic) = apply(&mut state, event) {
            state.broken = Some(diagnostic);
            break;
        }
    }

    state
}

fn apply(state: &mut RunState, event: &Event) -> Result<(), String> {
    match &event.payload {
        EventPayload::NodeStarted(p) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                None | Some(NodeState::Failed { .. }) => {
                    state
                        .nodes
                        .insert(node_id, NodeState::Running { attempt: p.attempt });
                    Ok(())
                }
                Some(_) => Err(format!(
                    "seq {}: node `{node_id}` got node_started while already running/finished",
                    event.seq
                )),
            }
        }
        EventPayload::NodeFinished(p) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                Some(NodeState::Running { .. }) => {
                    state.total_tokens = sum_tokens(state.total_tokens, p.tokens_used);
                    state.nodes.insert(
                        node_id,
                        NodeState::Finished {
                            outcome: p.outcome.clone(),
                            tokens: p.tokens_used,
                        },
                    );
                    Ok(())
                }
                _ => Err(format!(
                    "seq {}: node `{node_id}` got node_finished without a matching node_started",
                    event.seq
                )),
            }
        }
        EventPayload::NodeFailed(p) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                Some(NodeState::Running { .. }) => {
                    state.total_tokens = sum_tokens(state.total_tokens, p.tokens_used);
                    state.nodes.insert(
                        node_id,
                        NodeState::Failed {
                            outcome: p.outcome.clone(),
                            tokens: p.tokens_used,
                            retryable: p.retryable,
                        },
                    );
                    Ok(())
                }
                _ => Err(format!(
                    "seq {}: node `{node_id}` got node_failed without a matching node_started",
                    event.seq
                )),
            }
        }
        EventPayload::TaskRegistered(p) => {
            state
                .tasks
                .entry(p.task_id.clone())
                .or_insert(TaskStatus::Pending);
            Ok(())
        }
        EventPayload::TaskStatusChanged(p) => {
            if !state.tasks.contains_key(&p.task_id) {
                return Err(format!(
                    "seq {}: task `{}` got task_status_changed without a prior task_registered",
                    event.seq, p.task_id
                ));
            }
            state.tasks.insert(p.task_id.clone(), p.new_status);
            Ok(())
        }
        // Every other kind is either run-scoped bookkeeping that does not
        // change node/task/budget state (runner_resolved, baseline_captured,
        // agent_session_opened, agent_message, artifact_written,
        // context_assembled, criteria_checked, scope_checked, scope
        // expansion, hook_executed, node_rerouted, promotion_signaled,
        // capability_degraded, run_paused/resumed/finished), or belongs to
        // schema M-0 doesn't have yet (gate_waiting/resolved, loop_iteration
        // beyond what tasks already cover, questions_answered, finding_posted,
        // child_run_*). Nothing to derive from them until their own task
        // adds the state they'd feed.
        _ => Ok(()),
    }
}

fn require_node_id(event: &Event) -> Result<NodeId, String> {
    event.node_id.clone().ok_or_else(|| {
        format!(
            "seq {}: `{}` is missing node_id",
            event.seq,
            event.payload.kind_name()
        )
    })
}

fn sum_tokens(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input: a.input + b.input,
        output: a.output + b.output,
        cached: match (a.cached, b.cached) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        },
    }
}

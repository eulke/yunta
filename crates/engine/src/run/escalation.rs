//! §5.3's escalation object, built once and shared (M8/T8.1). Two
//! constructors — one per gate shape this recorte covers — used by both
//! the live pause path (`run/mod.rs`'s `GateExhaustedReroutes` arm and
//! `gate_exec::resolve_internal_gate`, which await a `HumanInteraction`
//! with the built object synchronously) and [`current_escalation`]
//! (which rebuilds the identical object for a run already paused, no
//! live process involved) — one construction site each, not two copies
//! that could drift apart.

use yunta_core::events::{Event, EventPayload, GateOption, GateWaitingPayload};
use yunta_core::{Manifest, NodeId, NodeKind, Workflow};

use super::schedule::{self, ScheduleStep};

/// §5.3's object for a node whose re-routes are exhausted (§11.2):
/// retry once more, abort, or — when `modes:` has somewhere later to go
/// — promote.
pub(crate) fn build_reroute_escalation(
    workflow: &Workflow,
    mode_name: &str,
    node: &NodeId,
    goto: &NodeId,
    max_reroutes: u32,
    cause: &str,
) -> GateWaitingPayload {
    let suggested_mode = schedule::next_mode_after(workflow, mode_name);
    let mut options = vec![
        GateOption {
            id: "retry".to_string(),
            label: format!("Re-route to `{goto}` once more"),
            tradeoff: format!(
                "Uses one extra correction attempt beyond the declared max_reroutes \
                 ({max_reroutes}); escalates again if `{goto}` doesn't fix it"
            ),
        },
        GateOption {
            id: "abort".to_string(),
            label: "Abort the run".to_string(),
            tradeoff: "Stops here; nothing further executes".to_string(),
        },
    ];
    if let Some(next_mode) = &suggested_mode {
        options.push(GateOption {
            id: "promote".to_string(),
            label: format!("Promote to mode `{next_mode}`"),
            tradeoff: format!(
                "Closes this run (`run_finished: promoted`) and starts a successor in \
                 `{next_mode}`, inheriting this run's artifacts; §10.2 — there's no \
                 mechanism to demote back to `{mode_name}`"
            ),
        });
    }
    GateWaitingPayload {
        summary: format!(
            "node `{node}` failed and its {max_reroutes} re-route(s) to `{goto}` are \
             exhausted: {cause}"
        ),
        evidence: cause.to_string(),
        options,
        external_ref: None,
    }
}

/// §5.3's object for an unresolved internal gate (`kind: gate`,
/// `external: None`, DI-04): its declared options (default: a single
/// `approve`), each with a tradeoff derived from its own `on:` mapping,
/// plus the engine's own `abort` unless the author already claimed
/// that id.
pub(crate) fn build_internal_gate_escalation(
    node: &NodeId,
    assignee: &str,
    message: Option<&str>,
    options: &[String],
    on: &indexmap::IndexMap<String, NodeId>,
) -> GateWaitingPayload {
    let declared: Vec<String> = if options.is_empty() {
        vec!["approve".to_string()]
    } else {
        options.to_vec()
    };
    let mut gate_options: Vec<GateOption> = declared
        .iter()
        .map(|id| GateOption {
            id: id.clone(),
            label: id.clone(),
            tradeoff: match on.get(id) {
                Some(target) => {
                    format!("re-routes to `{target}` and asks again once it completes")
                }
                None => "resolves this gate; the flow continues".to_string(),
            },
        })
        .collect();
    let engine_abort = !declared.iter().any(|id| id == "abort");
    if engine_abort {
        gate_options.push(GateOption {
            id: "abort".to_string(),
            label: "Abort the run".to_string(),
            tradeoff: "Pauses here; nothing further executes".to_string(),
        });
    }
    GateWaitingPayload {
        summary: message
            .map(str::to_string)
            .unwrap_or_else(|| format!("gate `{node}` needs a decision")),
        evidence: format!("assignee: {assignee}"),
        options: gate_options,
        external_ref: None,
    }
}

/// M8/T8.1: reconstructs the §5.3 escalation object a paused run is
/// currently waiting on, purely from the manifest and its own log — no
/// live process required. This is what lets `resolve_gate` (a `yunta
/// mcp` tool call, running in a process that never paused this run)
/// know what it's answering: `schedule::next_step` is pure, so calling
/// it again on the same log deterministically reaches the same
/// `GateExhaustedReroutes`/`ResolveInternalGate` step the paused
/// invocation saw — same inputs, same escalation, even though nothing
/// was ever logged for the "no live surface" case (§5.3's "el que
/// escala hace el trabajo de armar la decisión" is a computation, not a
/// persisted fact, until a human actually answers).
///
/// `None` covers every pause this function's two cases don't: a plain
/// failure with no `on_failure`, a budget cap, a cancellation, an
/// external gate degraded to console for lack of a forge (deliberately
/// out of scope here — whether a forge is reachable depends on the
/// calling machine's own environment, not on the log, so it isn't a
/// pure question `manifest`+`events` alone can answer). Those pauses
/// have no menu to reconstruct; the caller keeps its existing reason.
pub fn current_escalation(manifest: &Manifest, events: &[Event]) -> Option<GateWaitingPayload> {
    let mode_name = match events.first().map(|e| &e.payload) {
        Some(EventPayload::RunCreated(p)) => p.mode.clone(),
        _ => return None,
    };
    let mode_nodes = schedule::mode_included_nodes(&manifest.workflow, &mode_name);
    let step = schedule::next_step(
        &manifest.workflow,
        events,
        manifest.max_parallel_nodes,
        manifest.config.resolved_on_interrupt(),
        mode_nodes.as_ref(),
    );
    match step {
        ScheduleStep::GateExhaustedReroutes {
            node,
            goto,
            max_reroutes,
            cause,
        } => Some(build_reroute_escalation(
            &manifest.workflow,
            &mode_name,
            &node,
            &goto,
            max_reroutes,
            &cause,
        )),
        ScheduleStep::ResolveInternalGate { node } => {
            let node = super::find_node(&manifest.workflow, &node).ok()?;
            let NodeKind::Gate {
                assignee,
                message,
                options,
                on,
                external: None,
            } = &node.kind
            else {
                return None;
            };
            Some(build_internal_gate_escalation(
                &node.id,
                assignee,
                message.as_deref(),
                options,
                on,
            ))
        }
        _ => None,
    }
}

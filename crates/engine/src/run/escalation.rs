//! The escalation object, built once and shared. Two
//! constructors — one per gate shape this recorte covers — used by both
//! the live pause path (`run/mod.rs`'s `GateExhaustedReroutes` arm and
//! `gate_exec::resolve_internal_gate`, which await a `HumanInteraction`
//! with the built object synchronously) and [`current_escalation`]
//! (which rebuilds the identical object for a run already paused, no
//! live process involved) — one construction site each, not two copies
//! that could drift apart.

use yunta_core::events::{EventDraft, EventPayload, GateOption, GateWaitingPayload, StoredEvent};
use yunta_core::{Manifest, ModeName, NodeId, NodeKind, RunId, Seq, Workflow};

use super::schedule::{self, ScheduleStep};
use crate::reserved::ReservedOption;

/// The escalation object for a node whose re-routes are exhausted:
/// retry once more, abort, or — when `modes:` has somewhere later to go
/// — promote.
pub(crate) fn build_reroute_escalation(
    workflow: &Workflow,
    mode_name: &ModeName,
    node: &NodeId,
    goto: &NodeId,
    max_reroutes: u32,
    cause: &str,
) -> GateWaitingPayload {
    let suggested_mode = schedule::next_mode_after(workflow, mode_name);
    let mut options = vec![
        GateOption {
            id: ReservedOption::Retry.as_str().to_string(),
            label: format!("Re-route to `{goto}` once more"),
            tradeoff: format!(
                "Uses one extra correction attempt beyond the declared max_reroutes \
                 ({max_reroutes}); escalates again if `{goto}` doesn't fix it"
            ),
        },
        GateOption {
            id: ReservedOption::Abort.as_str().to_string(),
            label: "Abort the run".to_string(),
            tradeoff: "Stops here; nothing further executes".to_string(),
        },
    ];
    if let Some(next_mode) = &suggested_mode {
        options.push(GateOption {
            id: ReservedOption::Promote.as_str().to_string(),
            label: format!("Promote to mode `{next_mode}`"),
            tradeoff: format!(
                "Closes this run (`run_finished: promoted`) and starts a successor in \
                 `{next_mode}`, inheriting this run's artifacts — there's no \
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

/// The escalation object for an unresolved internal gate (`kind: gate`,
/// `external: None`): its declared options (default: a single
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
        vec![ReservedOption::Approve.as_str().to_string()]
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
    let engine_abort = !declared
        .iter()
        .any(|id| id == ReservedOption::Abort.as_str());
    if engine_abort {
        gate_options.push(GateOption {
            id: ReservedOption::Abort.as_str().to_string(),
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

/// Reconstructs the escalation object a paused run is
/// currently waiting on, purely from the manifest and its own log — no
/// live process required. This is what lets `resolve_gate` (a `yunta
/// mcp` tool call, running in a process that never paused this run)
/// know what it's answering: `schedule::next_step` is pure, so calling
/// it again on the same log deterministically reaches the same
/// `GateExhaustedReroutes`/`ResolveInternalGate` step the paused
/// invocation saw — same inputs, same escalation, even though nothing
/// was ever logged for the "no live surface" case (the one who escalates
/// does the work of building the decision — that's a computation, not
/// a persisted fact, until a human actually answers).
///
/// `None` covers every pause this function's two cases don't: a plain
/// failure with no `on_failure`, a budget cap, a cancellation, an
/// external gate degraded to console for lack of a forge (deliberately
/// out of scope here — whether a forge is reachable depends on the
/// calling machine's own environment, not on the log, so it isn't a
/// pure question `manifest`+`events` alone can answer). Those pauses
/// have no menu to reconstruct; the caller keeps its existing reason.
///
/// The `NodeId` alongside the object is the node the decision belongs
/// to — `resolve_gate` needs it to record `gate_waiting`/`gate_resolved`
/// against the right node, the same one the live pause path would have
/// used.
pub fn current_escalation(
    manifest: &Manifest,
    events: &[StoredEvent],
) -> Option<(NodeId, GateWaitingPayload)> {
    let mode_name = current_mode_name(events)?;
    match current_step(manifest, events)? {
        ScheduleStep::GateExhaustedReroutes {
            node,
            goto,
            max_reroutes,
            cause,
        } => {
            let escalation = build_reroute_escalation(
                &manifest.workflow,
                &mode_name,
                &node,
                &goto,
                max_reroutes,
                &cause,
            );
            Some((node, escalation))
        }
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
            let escalation =
                build_internal_gate_escalation(&node.id, assignee, message.as_deref(), options, on);
            Some((node.id.clone(), escalation))
        }
        _ => None,
    }
}

fn current_mode_name(events: &[StoredEvent]) -> Option<ModeName> {
    match events.first().and_then(StoredEvent::payload) {
        Some(EventPayload::RunCreated(p)) => Some(p.mode.clone()),
        _ => None,
    }
}

/// The raw scheduler decision behind [`current_escalation`] — `None`
/// for every step that isn't one of the two gate shapes.
fn current_step(manifest: &Manifest, events: &[StoredEvent]) -> Option<ScheduleStep> {
    let mode_name = current_mode_name(events)?;
    let mode_nodes = crate::modes::mode_included_nodes(&manifest.workflow, &mode_name);
    let step = schedule::next_step(
        &manifest.workflow,
        events,
        manifest.max_parallel_nodes,
        manifest.config.resolved_on_interrupt(),
        mode_nodes.as_ref(),
    );
    match step {
        ScheduleStep::GateExhaustedReroutes { .. } | ScheduleStep::ResolveInternalGate { .. } => {
            Some(step)
        }
        _ => None,
    }
}

/// Answers the escalation a paused run is currently waiting on
/// by appending **only the decision** to its log — the same pair
/// (`gate_waiting`, so the object the human saw is auditable, plus
/// `gate_resolved`). The *consequence* is never written here: the next
/// engine to wake this run (the detached `yunta resume` the caller
/// spawns) finds the pre-seeded decision via [`pre_seeded_resolution`]
/// and applies it through its one existing consequence path — retry,
/// abort, promote (a live process, exactly what promotion's
/// distill+successor needs) and internal gates alike, with zero
/// duplicated consequence logic that could drift.
///
/// Preconditions: the run must be parked (its last event is
/// `run_paused` — refuses `NotPaused` otherwise, which also removes any
/// race with a live process about to ask the same question), and the
/// chosen option must be on the reconstructed escalation's own menu.
pub async fn resolve_gate(
    manifest: &Manifest,
    storage: &yunta_storage::AsyncStorage,
    run_id: &RunId,
    clock: &dyn yunta_core::Clock,
    option_id: &str,
    resolved_by: Option<String>,
    free_text: Option<String>,
) -> Result<(), ResolveGateError> {
    let events = storage.events_for_run(run_id.clone()).await?;
    if !matches!(
        events.last().and_then(StoredEvent::payload),
        Some(EventPayload::RunPaused(_))
    ) {
        return Err(ResolveGateError::NotPaused);
    }
    let Some((node, escalation)) = current_escalation(manifest, &events) else {
        return Err(ResolveGateError::NothingToResolve);
    };
    if !escalation.options.iter().any(|o| o.id == option_id) {
        return Err(ResolveGateError::UnknownOption {
            chosen: option_id.to_string(),
            declared: escalation
                .options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    let resolution = yunta_core::events::GateResolvedPayload {
        chosen_option: Some(option_id.to_string()),
        resolved_by,
        free_text,
        approved_sha: None,
    };
    storage
        .append(
            EventDraft {
                run_id: run_id.clone(),
                node_id: Some(node.clone()),
                payload: EventPayload::GateWaiting(escalation),
            },
            clock.now(),
        )
        .await?;
    storage
        .append(
            EventDraft {
                run_id: run_id.clone(),
                node_id: Some(node),
                payload: EventPayload::GateResolved(resolution),
            },
            clock.now(),
        )
        .await?;
    Ok(())
}

/// The pre-seeded decision waiting for `node`, if one qualifies
/// — pure over the log. The latest `gate_resolved` for the node
/// qualifies iff its seq is greater than the node's last
/// `node_failed`/`node_rerouted`/`node_finished` **and** the run's last
/// `run_paused`. That makes exactly the right pairs qualify: one
/// appended by [`resolve_gate`] while parked (nothing after it); never
/// a live-path pair (its consequence — a node event, or the abort's own
/// `run_paused` — always lands right after); never a consumed abort (a
/// fresh `run_paused` follows it, so a later manual resume asks again,
/// today's exact semantics); never a decision from before a newer
/// failure. Callers still re-validate the chosen option against the
/// re-derived menu — a mismatch means ask normally, never guess.
pub(crate) fn pre_seeded_resolution(
    events: &[StoredEvent],
    node: &NodeId,
) -> Option<yunta_core::events::GateResolvedPayload> {
    let mut latest: Option<(Seq, yunta_core::events::GateResolvedPayload)> = None;
    let mut blocker: Option<Seq> = None;
    for event in events {
        match event.payload() {
            Some(EventPayload::GateResolved(p)) if event.node_id.as_ref() == Some(node) => {
                latest = Some((event.seq, p.clone()));
            }
            Some(
                EventPayload::NodeFailed(_)
                | EventPayload::NodeRerouted(_)
                | EventPayload::NodeFinished(_),
            ) if event.node_id.as_ref() == Some(node) => {
                blocker = blocker.max(Some(event.seq));
            }
            Some(EventPayload::RunPaused(_)) => blocker = blocker.max(Some(event.seq)),
            _ => {}
        }
    }
    latest
        .filter(|(seq, _)| Some(*seq) > blocker)
        .map(|(_, resolution)| resolution)
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveGateError {
    #[error(
        "this run isn't parked at a pause — a live process may still be driving it (or it \
         already finished); check `yunta status` and try again once it's paused"
    )]
    NotPaused,
    #[error(
        "this run isn't currently waiting on a decision `resolve_gate` can answer — it's \
         paused for a reason with no menu of options (a plain failure, a budget cap, an \
         external gate with no forge)"
    )]
    NothingToResolve,
    #[error("option `{chosen}` isn't valid here — declared options: {declared}")]
    UnknownOption { chosen: String, declared: String },
    #[error(transparent)]
    Storage(#[from] yunta_storage::StorageError),
}

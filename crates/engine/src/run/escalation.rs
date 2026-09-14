//! The escalation object, built once and shared. Two
//! constructors — one per gate shape — used by both
//! the live pause path (`run/mod.rs`'s `GateExhaustedReroutes` arm and
//! `gate_exec::resolve_internal_gate`, which await a `HumanInteraction`
//! with the built object synchronously) and [`current_escalation`]
//! (which rebuilds the identical object for a run already paused, no
//! live process involved) — one construction site each, not two copies
//! that could drift apart.

use yunta_core::events::{
    Escalation, EscalationError, EventDraft, EventPayload, Fact, GateOption, GateResolvedPayload,
    GateWaitingPayload, HumanChoice, StoredEvent,
};
use yunta_core::{Manifest, ModeName, NodeId, NodeKind, NonEmpty, OptionId, RunId, Workflow};

use super::schedule::{self, Decision};
use crate::replay::RunState;
use crate::reserved::{offers, ReservedOption};
use yunta_core::events::{GateEvent, RerouteCause, RunEvent};

/// Whether an event is the run-level `run_paused` marker — the one predicate
/// the resolve-gate path reads a parked run's log by.
fn is_run_paused(event: &StoredEvent) -> bool {
    matches!(
        event.payload(),
        Some(EventPayload::Run(RunEvent::Paused(_)))
    )
}

/// The escalation object for a node whose re-routes are exhausted:
/// retry once more, abort, or — when `modes:` has somewhere later to go
/// — promote.
pub(crate) fn build_reroute_escalation(
    workflow: &Workflow,
    mode_name: &ModeName,
    node: &NodeId,
    goto: &NodeId,
    max_reroutes: u32,
    cause: &RerouteCause,
) -> Result<Escalation, EscalationError> {
    let suggested_mode = schedule::next_mode_after(workflow, mode_name);
    let retry = offers::retry(goto, max_reroutes);
    let mut rest = vec![offers::abort()];
    if let Some(next_mode) = &suggested_mode {
        rest.push(offers::promote(next_mode, mode_name));
    }
    // The cause names itself — `exit 1` needs no word in front of it —
    // so it is attached as the record, not repeated into the claim
    // above it.
    Escalation::new(
        format!(
            "node `{node}` failed and its {max_reroutes} re-route(s) to `{goto}` are exhausted"
        ),
        vec![Fact::bare(cause.to_string())].into(),
        NonEmpty::from((retry, rest)),
    )
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
    options: &[OptionId],
    on: &indexmap::IndexMap<OptionId, NodeId>,
) -> Result<Escalation, EscalationError> {
    let declared: Vec<OptionId> = if options.is_empty() {
        vec![ReservedOption::Approve.id()]
    } else {
        options.to_vec()
    };
    let mut gate_options: Vec<GateOption> = declared
        .iter()
        .map(|id| offers::declared(id, on.get(id)))
        .collect();
    let engine_abort = !declared
        .iter()
        .any(|id| ReservedOption::of(id) == Some(ReservedOption::Abort));
    if engine_abort {
        gate_options.push(offers::abort());
    }
    // An author's own `message:` is the claim; the assignee is the
    // record of who it is addressed to.
    let (first, rest) = gate_options
        .split_first()
        .map(|(first, rest)| (first.clone(), rest.to_vec()))
        .unwrap_or_else(|| (offers::abort(), Vec::new()));
    Escalation::new(
        message
            .map(str::to_string)
            .unwrap_or_else(|| format!("gate `{node}` needs a decision")),
        vec![Fact::labelled("assignee", assignee)].into(),
        NonEmpty::from((first, rest)),
    )
}

/// Reconstructs the escalation object a paused run is
/// currently waiting on, purely from the manifest and its own log — no
/// live process required. This is what lets `resolve_gate` (a `yunta
/// mcp` tool call, running in a process that never paused this run)
/// know what it's answering: `schedule::decide` is pure, so asking it
/// again about the same state deterministically reaches the same
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
pub fn current_escalation(manifest: &Manifest, state: &RunState) -> Option<(NodeId, Escalation)> {
    let mode_name = state.run.mode().clone();
    match current_decision(manifest, state, &mode_name)? {
        Decision::GateExhaustedReroutes {
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
            )
            .ok()?;
            Some((node, escalation))
        }
        Decision::ResolveInternalGate { node } => {
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
                build_internal_gate_escalation(&node.id, assignee, message.as_deref(), options, on)
                    .ok()?;
            Some((node.id.clone(), escalation))
        }
        _ => None,
    }
}

/// The raw scheduler decision behind [`current_escalation`] — `None`
/// for every decision that isn't one of the two gate shapes.
fn current_decision(
    manifest: &Manifest,
    state: &RunState,
    mode_name: &ModeName,
) -> Option<Decision> {
    let decision = schedule::decide(
        &manifest.workflow,
        state,
        &schedule::Policy::of(manifest, mode_name),
    );
    match decision {
        Decision::GateExhaustedReroutes { .. } | Decision::ResolveInternalGate { .. } => {
            Some(decision)
        }
        _ => None,
    }
}

/// Answers the escalation a paused run is currently waiting on
/// by appending **only the decision** to its log — the same pair
/// (`gate_waiting`, so the object the human saw is auditable, plus
/// `gate_resolved`). The *consequence* is never written here: the next
/// engine to wake this run (the detached `yunta resume` the caller
/// spawns) finds the pre-seeded decision via `pre_seeded_resolution`
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
    choice: HumanChoice,
) -> Result<(), ResolveGateError> {
    let events = storage.events_for_run(run_id.clone()).await?;
    if !events.last().is_some_and(is_run_paused) {
        return Err(ResolveGateError::NotPaused);
    }
    let Some((node, escalation)) = current_escalation(manifest, &crate::replay::derive(&events))
    else {
        return Err(ResolveGateError::NothingToResolve);
    };
    if !escalation.offers(&choice.option) {
        return Err(ResolveGateError::UnknownOption {
            chosen: choice.option,
            declared: escalation.menu(),
        });
    }
    let resolution = GateResolvedPayload::Chosen(choice);
    storage
        .append(
            EventDraft {
                run_id: run_id.clone(),
                node_id: Some(node.clone()),
                payload: EventPayload::Gates(GateEvent::Waiting(escalation.into_payload())),
            },
            clock.now(),
        )
        .await?;
    storage
        .append(
            EventDraft {
                run_id: run_id.clone(),
                node_id: Some(node),
                payload: EventPayload::Gates(GateEvent::Resolved(resolution)),
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
/// failure. The decision also has to be a human's choice of an option
/// `escalation`'s re-derived menu still offers; anything else (an
/// option the menu dropped, a shape no surface produces) means ask
/// The decision a `resolve_gate` call seeded onto this node's log while
/// the run was parked, when the escalation it answers still offers it.
///
/// The window is [`RunState::pre_seeded`]'s to decide — it reads the
/// three ledgers that say whether anything consumed the decision — and
/// this adds the one thing that is not a fact of the log: whether the
/// menu the run would ask with now still has that option on it. A
/// mismatch means the escalation changed under the answer, and the run
/// asks again.
pub(crate) fn pre_seeded_resolution(
    state: &RunState,
    node: &NodeId,
    escalation: &GateWaitingPayload,
) -> Option<HumanChoice> {
    match state.pre_seeded(node)? {
        GateResolvedPayload::Chosen(choice) if escalation.offers(&choice.option) => {
            Some(choice.clone())
        }
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveGateError {
    /// The sentence names the state the run is in and stops there:
    /// which command shows a reader where that run stands is the
    /// caller's own vocabulary, not the engine's.
    #[error(
        "this run isn't parked at a pause — a live process may still be driving it, or it \
         already finished"
    )]
    NotPaused,
    #[error(
        "this run isn't currently waiting on a decision `resolve_gate` can answer — it's \
         paused for a reason with no menu of options (a plain failure, a budget cap, an \
         external gate with no forge)"
    )]
    NothingToResolve,
    #[error("option `{chosen}` isn't valid here — declared options: {declared}")]
    UnknownOption { chosen: OptionId, declared: String },
    #[error(transparent)]
    Storage(#[from] yunta_storage::StorageError),
}

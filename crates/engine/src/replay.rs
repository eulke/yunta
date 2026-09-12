//! State derivation by replay — covers what the event schema can
//! currently produce.
//!
//! `derive` is the functional core: a pure function
//! over an event slice, no IO, safe to call from a property test or from
//! `yunta resume` alike. It tracks what the schema can actually produce
//! today — node lifecycle (`node_started`/`node_finished`/`node_failed`)
//! and task status (`task_registered`/`task_status_changed`) — plus the
//! running token total `limits.max_tokens_per_run` is compared
//! against. `waiting` is derived too: a published gate without its
//! resolution (the state that outlives an invocation), and a node whose
//! `kind: questions` artifact has no `questions_answered` yet.
//! The internal waiting+resolved pair, emitted together, round-trips
//! back to the node's prior state by construction.
//!
//! A log that is insufficient or inconsistent — e.g. `node_finished` for a
//! node that was never `node_started` — marks the result `broken` with a
//! diagnostic naming the exact event, rather than panicking or guessing.
//! Replay stops at the first such event; the state accumulated up to
//! that point is still returned.

use std::collections::HashMap;
use std::path::PathBuf;

use yunta_core::events::{EventPayload, Failure, Finding, StoredEvent, TaskStatus, TokenUsage};
use yunta_core::{NodeId, Seq, TaskId};

/// One node's derived lifecycle state. An enum, not booleans:
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
        /// Why the node failed, as the log recorded it. The prose a
        /// reader sees is produced from this (it is `Display`), so no
        /// surface can disagree with the facts behind it.
        failure: Failure,
        tokens: TokenUsage,
        /// Whether the caller that owned the budget expected another
        /// attempt at this node.
        retryable: bool,
    },
    /// Waiting on a human — a published, unresolved gate
    /// (`gate_waiting` with no `gate_resolved` after it), or a node
    /// whose `kind: questions` artifact has no `questions_answered`
    /// after it. `external_ref` is the forge's handle (a PR URL) for
    /// external gates, `None` for everything else.
    Waiting {
        external_ref: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunState {
    pub nodes: HashMap<NodeId, NodeState>,
    pub tasks: HashMap<TaskId, TaskStatus>,
    pub total_tokens: TokenUsage,
    /// Every finding that stands: the last state of each id nobody
    /// withdrew, in the order each was first posted. Never deduplicated
    /// here — two nodes that find the same thing each keep their own
    /// posting ("without losing authorship"); [`dedup_findings`] is the
    /// query-side view for counting and display.
    pub findings: Vec<Finding>,
    /// Every `artifact_written` path, grouped by the node that wrote it,
    /// in log order (`progress.md`'s own "what each node produced"). A
    /// node with no artifact has no entry here at all, not an empty
    /// `Vec`.
    pub artifacts: HashMap<NodeId, Vec<PathBuf>>,
    /// `Some(diagnostic)` once the log has proven insufficient to derive
    /// further state — the point where a `yunta resume`/`status` would
    /// report the run as `broken`.
    pub broken: Option<String>,
    /// Every event under a `kind` this binary does not know, by position
    /// and kind name, in log order: the run is interpreted up to what
    /// this binary understands, and what it skipped is named.
    pub unknown_kinds: Vec<(Seq, String)>,
}

/// A run's log read once and its [`RunState`] derived once — the pair
/// almost every read site needs together. The read and the [`derive`]
/// call live here, in one place, instead of being repeated at each site.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunView {
    pub(crate) events: Vec<StoredEvent>,
    pub(crate) state: RunState,
}

impl RunView {
    /// Derives the state from the events, keeping both.
    pub(crate) fn of(events: Vec<StoredEvent>) -> Self {
        let state = derive(&events);
        Self { events, state }
    }
}

/// Bookkeeping `derive` needs across events without exposing it on
/// [`RunState`]: what a `Waiting` node was before its gate
/// opened (so `gate_resolved` can restore it — the internal
/// waiting+resolved pair leaves a `Failed` node `Failed`), and which
/// nodes have a `questions` artifact still unanswered (so their
/// `node_failed` derives `Waiting` — nodes whose pending questions await
/// an answer).
#[derive(Default)]
struct Aux {
    pre_gate: HashMap<NodeId, Option<NodeState>>,
    pending_questions: std::collections::HashSet<NodeId>,
    /// Folds the run's finding events, so `RunState.findings` is what
    /// stands rather than what was ever posted.
    findings: yunta_core::events::findings::FindingLedger,
}

/// How many events a log carries under one `kind` this binary does not
/// know — what `status`, the receipt and `stats` show so a partially
/// interpreted run is never mistaken for a fully interpreted one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UnknownKindCount {
    pub kind: String,
    pub events: usize,
}

/// The unknown kinds a derived state skipped, grouped by kind and sorted
/// by name; empty for a log this binary interprets in full.
pub fn unknown_kind_counts(state: &RunState) -> Vec<UnknownKindCount> {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (_, kind) in &state.unknown_kinds {
        *counts.entry(kind.as_str()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(kind, events)| UnknownKindCount {
            kind: kind.to_string(),
            events,
        })
        .collect()
}

/// Derives run state from its event log, in `seq` order. Pure: same
/// input, same output, always — a property test relies on exactly
/// this.
pub fn derive(events: &[StoredEvent]) -> RunState {
    let mut state = RunState::default();
    let mut aux = Aux::default();

    for event in events {
        if let Err(error) = apply(&mut state, &mut aux, event) {
            state.broken = Some(error.to_string());
            break;
        }
    }

    state
}

fn apply(state: &mut RunState, aux: &mut Aux, event: &StoredEvent) -> Result<(), ReplayError> {
    let Some(payload) = event.payload() else {
        // A kind this binary does not know: counted, named, never a
        // reason to stop deriving what it does know.
        state
            .unknown_kinds
            .push((event.seq, event.body.kind_name().to_string()));
        return Ok(());
    };
    match payload {
        EventPayload::NodeStarted(p) => {
            let node_id = require_node_id(event)?;
            // Any prior state is a legal starting point: a `Failed` node
            // re-runs after its re-route resolves, a `Finished`
            // corrective node re-runs on the next re-route to it, and a
            // `Running` node restarts when resume finds it orphaned
            // (`restart_node`). The log records what happened; the
            // attempt number carries the history.
            state
                .nodes
                .insert(node_id, NodeState::Running { attempt: p.attempt });
            Ok(())
        }
        EventPayload::NodeFinished(p) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                Some(NodeState::Running { .. }) => {
                    state.total_tokens += p.tokens_used;
                    state.nodes.insert(
                        node_id,
                        NodeState::Finished {
                            outcome: p.outcome.clone(),
                            tokens: p.tokens_used,
                        },
                    );
                    Ok(())
                }
                _ => Err(ReplayError::FinishedWithoutStart {
                    seq: event.seq,
                    node: node_id,
                }),
            }
        }
        EventPayload::NodeFailed(p) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                Some(NodeState::Running { .. }) => {
                    state.total_tokens += p.tokens_used;
                    // A node that failed *because its
                    // questions are unanswered* is `waiting`, not
                    // `failed` — the questions artifact preceding it
                    // (with no `questions_answered` since) is the typed
                    // signal, never the diagnostic string.
                    let next = if aux.pending_questions.contains(&node_id) {
                        NodeState::Waiting { external_ref: None }
                    } else {
                        NodeState::Failed {
                            failure: p.failure.clone(),
                            tokens: p.tokens_used,
                            retryable: p.retryable,
                        }
                    };
                    state.nodes.insert(node_id, next);
                    Ok(())
                }
                _ => Err(ReplayError::FailedWithoutStart {
                    seq: event.seq,
                    node: node_id,
                }),
            }
        }
        EventPayload::GateWaiting(p) => {
            // No node = a run-level escalation (the token budget check):
            // it gates the whole invocation, not any node's
            // state, so derivation records nothing for it.
            if event.node_id.is_none() {
                return Ok(());
            }
            let node_id = require_node_id(event)?;
            // A published (or console-rendered-and-resolved-next)
            // gate: the node is waiting on a human from this point until
            // `gate_resolved`. What it was before is remembered so the
            // synchronous internal pair restores it exactly.
            aux.pre_gate
                .insert(node_id.clone(), state.nodes.get(&node_id).cloned());
            state.nodes.insert(
                node_id,
                NodeState::Waiting {
                    external_ref: p.external_ref.clone(),
                },
            );
            Ok(())
        }
        EventPayload::GateResolved(_) => {
            // Run-level resolution (see `GateWaiting` above): audited in
            // the log, invisible to node state.
            if event.node_id.is_none() {
                return Ok(());
            }
            let node_id = require_node_id(event)?;
            // Only restores while still `Waiting`: an external gate's
            // poll resolution emits `node_started` *before*
            // `gate_resolved`, so by the time this arrives the node is
            // already `Running` and the outcome events own its state.
            if matches!(state.nodes.get(&node_id), Some(NodeState::Waiting { .. })) {
                match aux.pre_gate.remove(&node_id).flatten() {
                    Some(prior) => {
                        state.nodes.insert(node_id, prior);
                    }
                    None => {
                        state.nodes.remove(&node_id);
                    }
                }
            }
            Ok(())
        }
        EventPayload::QuestionsAnswered(_) => {
            let node_id = require_node_id(event)?;
            aux.pending_questions.remove(&node_id);
            // If the node was already derived `Waiting` on those
            // questions (a resume answering them), the answer alone
            // doesn't finish it — the caller emits `node_started` +
            // `node_finished` around it, which own the state transition.
            Ok(())
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
                return Err(ReplayError::StatusWithoutTask {
                    seq: event.seq,
                    task: p.task_id.clone(),
                });
            }
            state.tasks.insert(p.task_id.clone(), p.new_status);
            Ok(())
        }
        // The three finding events fold together or not at all: what a
        // run holds is the last state of every id nobody withdrew, and
        // `FindingLedger` is the one place that says so.
        EventPayload::FindingPosted(_)
        | EventPayload::FindingUpdated(_)
        | EventPayload::FindingWithdrawn(_) => {
            aux.findings.apply(event.node_id.as_ref(), payload);
            state.findings = aux
                .findings
                .effective()
                .into_iter()
                .map(|posted| posted.finding)
                .collect();
            Ok(())
        }
        EventPayload::ArtifactWritten(p) => {
            let node_id = require_node_id(event)?;
            if p.artifact_kind == Some(yunta_core::ArtifactKind::Questions) {
                aux.pending_questions.insert(node_id.clone());
            }
            state
                .artifacts
                .entry(node_id)
                .or_default()
                .push(p.path.clone());
            Ok(())
        }
        EventPayload::ChildRunFinished(p) => {
            // The child's whole spend aggregates into the
            // parent's total right here — once per chain member, at its
            // close; the parent node's own `node_finished` deliberately
            // carries none of it (see the payload's doc).
            state.total_tokens += p.tokens;
            Ok(())
        }
        // Every other kind is run-scoped bookkeeping that does not
        // change node/task/budget state (runner_resolved, baseline_captured,
        // agent_session_opened, agent_message,
        // context_assembled, criteria_checked, scope_checked, scope
        // expansion, hook_executed, node_rerouted, promotion_signaled,
        // capability_degraded, run_paused/resumed/finished, loop_iteration
        // beyond what tasks already cover). child_run_created/finished
        // deliberately included: the parent node's own
        // started/finished/failed events carry its derived state (child
        // tokens aggregate through node_finished.tokens_used), while the
        // link pair stays pure audit — `workflow_exec` reads it directly
        // off the log to find an open child, no derived field needed
        // (its tokens are handled in the arm above).
        _ => Ok(()),
    }
}

/// Query-side view of `RunState.findings`: findings across reviewers are
/// deduplicated by `location` plus normalized title. The raw log (and
/// `RunState.findings`) keeps every posting; this collapses duplicates
/// for counting/display, keeping the first occurrence — the schema has
/// no authors list to merge into, so authorship is preserved by the
/// untouched event log, not by this derived view.
pub fn dedup_findings(findings: &[Finding]) -> Vec<Finding> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for finding in findings {
        let key = (
            finding.location.clone(),
            finding.title.trim().to_lowercase(),
        );
        if seen.insert(key) {
            deduped.push(finding.clone());
        }
    }
    deduped
}

fn require_node_id(event: &StoredEvent) -> Result<NodeId, ReplayError> {
    event
        .node_id
        .clone()
        .ok_or_else(|| ReplayError::MissingNodeId {
            seq: event.seq,
            kind: event.body.kind_name().to_string(),
        })
}

/// The point at which the log stops making sense — what
/// [`RunState::broken`] reports.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum ReplayError {
    #[error("seq {seq}: `{kind}` is missing node_id")]
    MissingNodeId { seq: Seq, kind: String },
    #[error("seq {seq}: node `{node}` got node_finished without a matching node_started")]
    FinishedWithoutStart { seq: Seq, node: NodeId },
    #[error("seq {seq}: node `{node}` got node_failed without a matching node_started")]
    FailedWithoutStart { seq: Seq, node: NodeId },
    #[error("seq {seq}: task `{task}` got task_status_changed without a prior task_registered")]
    StatusWithoutTask { seq: Seq, task: TaskId },
}

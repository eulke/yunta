//! State derivation by replay — covers what the event schema can
//! currently produce.
//!
//! `derive` is the functional core: a pure function
//! over an event slice, no IO, safe to call from a property test or from
//! `yunta resume` alike. It tracks what the schema can actually produce
//! today — node lifecycle (`node_started`/`node_finished`/`node_failed`)
//! task status and the node each task belongs to
//! (`task_registered`/`task_status_changed`) — plus the
//! running token total `limits.max_tokens_per_run` is compared
//! against. `waiting` is derived too: a published gate without its
//! resolution (the state that outlives an invocation), and a node that
//! recorded `questions_asked` with no `questions_answered` yet. Both are
//! the same shape — a fact that opens the wait and a fact that closes
//! it — and both round-trip back to the node's prior state by
//! construction, so a node that asked comes back `Running`, owed the
//! terminal its close deferred.
//!
//! A log that is insufficient or inconsistent — e.g. `node_finished` for a
//! node that was never `node_started` — marks the result `broken` with a
//! diagnostic naming the exact event, rather than panicking or guessing.
//! Replay stops at the first such event; the state accumulated up to
//! that point is still returned.

use std::collections::HashMap;

use yunta_core::events::artifacts::ArtifactLedger;
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
    /// (`gate_waiting` with no `gate_resolved` after it), or a node that
    /// asked (`questions_asked` with no `questions_answered` after it).
    /// `external_ref` is the forge's handle (a PR URL) for external
    /// gates, `None` for everything else.
    Waiting {
        external_ref: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunState {
    pub nodes: HashMap<NodeId, NodeState>,
    pub tasks: HashMap<TaskId, TaskStatus>,
    /// The node each task belongs to: the one whose events registered it
    /// or moved its status. Two `loop` nodes running at once each
    /// register their own tasks, and this is what tells the two sets
    /// apart — without it a task is a bare id with no node to show it
    /// under. A task no event attributes to a node has no entry here at
    /// all.
    pub task_nodes: HashMap<TaskId, NodeId>,
    pub total_tokens: TokenUsage,
    /// Every finding that stands: the last state of each id nobody
    /// withdrew, in the order each was first posted. Never deduplicated
    /// here — two nodes that find the same thing each keep their own
    /// posting ("without losing authorship"); [`dedup_findings`] is the
    /// query-side view for counting and display.
    pub findings: Vec<Finding>,
    /// Every artifact the run holds, folded from its acceptances: what
    /// each one is, the hash of its bytes and how the run came by it.
    /// The one fold — a surface that lists, resolves or counts artifacts
    /// reads it here rather than walking the log again.
    pub artifacts: ArtifactLedger,
    /// Every node whose questions were answered and whose close still
    /// owes it a terminal: `questions_answered` with no `node_started`,
    /// `node_finished` or `node_failed` after it.
    ///
    /// The node is `Running` again — its close already ran, when it
    /// asked — so nothing tells it apart from a node a crash orphaned
    /// except this. The scheduler pays the terminal from the log rather
    /// than restarting a session that already did its work.
    pub answered_unfinished: std::collections::BTreeSet<NodeId>,
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
/// [`RunState`]: what a `Waiting` node was before the fact that opened
/// its wait (so the fact that closes it can restore that state — the
/// internal waiting+resolved pair leaves a `Failed` node `Failed`, and a
/// node that asked comes back `Running`).
#[derive(Default)]
struct Aux {
    pre_gate: HashMap<NodeId, Option<NodeState>>,
    /// What the session that asked spent, held from `questions_asked`
    /// until the terminal that closes the node carries it: the attempt's
    /// accounting closes when it asks, and the node's total is still the
    /// whole attempt.
    asked_tokens: HashMap<NodeId, TokenUsage>,
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
    // Which kinds state an artifact is `ArtifactLedger`'s to know, and
    // its fold is total — so every event goes through it and this
    // derivation never names an artifact event at all. Whether a node
    // waits is a fact of its own (`questions_asked`), never something
    // deduced from the documents the run happens to hold.
    state
        .artifacts
        .apply(event.node_id.as_ref(), event.seq, payload);

    match payload {
        EventPayload::NodeStarted(p) => {
            let node_id = require_node_id(event)?;
            // Any prior state is a legal starting point: a `Failed` node
            // re-runs after its re-route resolves, a `Finished`
            // corrective node re-runs on the next re-route to it, and a
            // `Running` node restarts when resume finds it orphaned
            // (`restart_node`). The log records what happened; the
            // attempt number carries the history.
            // A fresh attempt owes nothing for an earlier round's
            // answer: whatever it produces closes it.
            state.answered_unfinished.remove(&node_id);
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
                    // A node that asked already paid for the session
                    // that asked, at `questions_asked`; what it carries
                    // is that plus whatever this terminal reports.
                    let asked = aux.asked_tokens.remove(&node_id).unwrap_or_default();
                    state.answered_unfinished.remove(&node_id);
                    state.nodes.insert(
                        node_id,
                        NodeState::Finished {
                            outcome: p.outcome.clone(),
                            tokens: asked + p.tokens_used,
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
                    // A failure is a failure, whatever documents the run
                    // holds for this node: a node waits because it
                    // recorded that it asked, never because a questions
                    // artifact exists.
                    let asked = aux.asked_tokens.remove(&node_id).unwrap_or_default();
                    state.answered_unfinished.remove(&node_id);
                    state.nodes.insert(
                        node_id,
                        NodeState::Failed {
                            failure: p.failure.clone(),
                            tokens: asked + p.tokens_used,
                            retryable: p.retryable,
                        },
                    );
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
        EventPayload::QuestionsAsked(p) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                Some(NodeState::Running { .. }) => {
                    // The attempt's accounting closes here: the session
                    // that asked is done, and no reader counts it again
                    // while the node waits.
                    state.total_tokens += p.tokens_used;
                    aux.asked_tokens.insert(node_id.clone(), p.tokens_used);
                    aux.pre_gate
                        .insert(node_id.clone(), state.nodes.get(&node_id).cloned());
                    state
                        .nodes
                        .insert(node_id, NodeState::Waiting { external_ref: None });
                    Ok(())
                }
                _ => Err(ReplayError::AskedWithoutStart {
                    seq: event.seq,
                    node: node_id,
                }),
            }
        }
        EventPayload::QuestionsAnswered(_) => {
            let node_id = require_node_id(event)?;
            match state.nodes.get(&node_id) {
                Some(NodeState::Waiting { .. }) => {
                    // The answer reopens the node exactly where asking
                    // left it: `Running`, owed the terminal its close
                    // deferred. The same shape as the internal
                    // gate pair, and the reason a crash here costs a
                    // `node_finished` rather than a whole session.
                    match aux.pre_gate.remove(&node_id).flatten() {
                        Some(prior) => {
                            state.nodes.insert(node_id.clone(), prior);
                        }
                        None => {
                            state.nodes.remove(&node_id);
                        }
                    }
                    state.answered_unfinished.insert(node_id);
                    Ok(())
                }
                _ => Err(ReplayError::AnsweredWithoutAsk {
                    seq: event.seq,
                    node: node_id,
                }),
            }
        }
        EventPayload::TaskRegistered(p) => {
            attribute_task(state, &p.task_id, event);
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
            attribute_task(state, &p.task_id, event);
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
        EventPayload::ChildRunFinished(p) => {
            // The child's whole spend aggregates into the
            // parent's total right here — once per chain member, at its
            // close; the parent node's own `node_finished` deliberately
            // carries none of it (see the payload's doc).
            state.total_tokens += p.tokens;
            Ok(())
        }
        // Every other kind is run-scoped bookkeeping that does not
        // change node/task/budget state (artifact_accepted — already
        // folded above, runner_resolved, baseline_captured,
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
        let key = (finding.location.clone(), normalized_title(&finding.title));
        if seen.insert(key) {
            deduped.push(finding.clone());
        }
    }
    deduped
}

/// Case- and whitespace-insensitive: "Scope  expansion DENIED" and
/// "scope expansion denied" are the same complaint about the same place.
fn normalized_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Records the node `event` attributes a task to, keeping the first one
/// the log names: a task belongs to the node that registered it, and a
/// later status change never re-homes it. An event with no node
/// attributes nothing — the task simply has no owner to show.
fn attribute_task(state: &mut RunState, task_id: &TaskId, event: &StoredEvent) {
    let Some(node_id) = &event.node_id else {
        return;
    };
    state
        .task_nodes
        .entry(task_id.clone())
        .or_insert_with(|| node_id.clone());
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
    #[error("seq {seq}: node `{node}` got questions_asked without a matching node_started")]
    AskedWithoutStart { seq: Seq, node: NodeId },
    #[error("seq {seq}: node `{node}` got questions_answered without a matching questions_asked")]
    AnsweredWithoutAsk { seq: Seq, node: NodeId },
    #[error("seq {seq}: task `{task}` got task_status_changed without a prior task_registered")]
    StatusWithoutTask { seq: Seq, task: TaskId },
}

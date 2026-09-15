//! State derivation by replay: a run's whole state read back off its
//! event log.
//!
//! `derive` is the functional core: a pure function over an event slice,
//! no IO, safe to call from a property test or from `yunta resume`
//! alike. It derives node lifecycle
//! (`node_started`/`node_finished`/`node_failed`), task status and the
//! node each task belongs to (`task_registered`/`task_status_changed`),
//! plus the running token total `limits.max_tokens_per_run` is compared
//! against. `waiting` is derived too: a published gate without its
//! resolution (the state that outlives an invocation), and a node that
//! recorded `questions_asked` with no `questions_answered` beside it.
//! Both are the same shape — a fact that opens the wait and one that
//! closes it — and both round-trip back to the node's prior state by
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
use yunta_core::events::findings::FindingLedger;
use yunta_core::events::tasks::ledger::UnknownTask;
use yunta_core::events::{
    ChildLedger, DegradationLedger, EventMeta, EventPayload, Finding, GateEvent, GateLedger,
    GateResolvedPayload, GrantLedger, NodeEvent, NodeLedger, RunLedger, StoredEvent, TaskLedger,
    TokenUsage,
};
use yunta_core::{NodeId, Seq, TaskId};

/// Re-exported where it has always been read from: the node states this
/// module derives are `yunta_core::events`' to define now, and every
/// caller keeps naming them here.
pub use yunta_core::events::NodeState;

/// Every fold of a run's log, in one value.
///
/// One ledger per domain, each the single answer to what its own kinds
/// mean. A surface that counts attempts, lists findings, asks what a
/// task is doing or where a gate stands reads the ledger rather than
/// walking the log with a rule of its own.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunState {
    pub run: RunLedger,
    pub nodes: NodeLedger,
    pub degradations: DegradationLedger,
    pub tasks: TaskLedger,
    pub grants: GrantLedger,
    pub findings: FindingLedger,
    pub artifacts: ArtifactLedger,
    pub gates: GateLedger,
    pub children: ChildLedger,
    /// Every event under a `kind` this binary does not know, by position
    /// and kind name, in log order: the run is interpreted up to what
    /// this binary understands, and what it skipped is named.
    pub unknown_kinds: Vec<(Seq, String)>,
    /// `Some(diagnostic)` once the log has proven insufficient to derive
    /// further state — the point where a `yunta resume`/`status` would
    /// report the run as `broken`.
    pub broken: Option<String>,
    /// What a node was before the fact that opened its wait, so the fact
    /// that closes it restores exactly that: the internal gate pair
    /// leaves a `Failed` node `Failed`, and a node that asked comes back
    /// `Running`.
    pre_gate: HashMap<NodeId, Option<NodeState>>,
}

impl RunState {
    /// Whether an invocation already woke this run.
    ///
    /// A birth writes as many events as the run was born holding — what
    /// the run is, the artifacts and tasks it starts with, the
    /// measurement its lineage handed it — and none of them is a wake.
    /// A wake leaves a pause, a resume or a measurement this run took;
    /// an invocation that died without writing its pause leaves only
    /// the nodes it started. What separates a first wake from a resume
    /// is what the log says happened, never how much of it there is.
    ///
    /// It reads more than one ledger because the question is about the
    /// whole log: each ledger answers for its own kinds, and this is
    /// the one place that reads them together.
    pub fn woken(&self) -> bool {
        self.run.woken()
            || !self.nodes.is_empty()
            // A log replay stopped on holds an event a birth never
            // writes; something worked on this run, whatever else is
            // true of what it left behind.
            || self.broken.is_some()
    }

    /// What the whole run has spent: every node's closed attempts, what
    /// is in flight, and every child run's own total.
    pub fn total_tokens(&self) -> TokenUsage {
        self.nodes
            .iter()
            .map(|(_, record)| record.tokens_closed)
            .sum::<TokenUsage>()
            + self.children.tokens()
    }

    /// Every finding that stands, in the order each was first posted.
    /// Never deduplicated here — two nodes that find the same thing each
    /// keep their own posting; [`dedup_findings`] is the query-side view
    /// for counting and display.
    pub fn effective_findings(&self) -> Vec<Finding> {
        self.findings
            .effective()
            .into_iter()
            .map(|posted| posted.finding)
            .collect()
    }

    /// Whether `node`'s questions were answered and its close still owes
    /// it a terminal.
    ///
    /// The node is `Running` again — its close already ran, when it
    /// asked — so nothing tells it apart from a node a crash orphaned
    /// except this.
    pub fn answered_unfinished(&self, node: &NodeId) -> bool {
        let answered = self.gates.answered(node);
        let owed = self.nodes.get(node).is_some_and(|record| {
            let answered_at = self
                .gates
                .get(node)
                .and_then(|gate| gate.rounds.last())
                .and_then(|round| round.answered_at);
            match (answered_at, record.last_terminal) {
                (Some(answered), Some(terminal)) => terminal < answered,
                (Some(_), None) => true,
                _ => false,
            }
        });
        answered && owed
    }

    /// Whether nothing has been derived from this state's log beyond
    /// the fact that a node was heard from — a node record carrying a
    /// timestamp and no derivation behind it.
    ///
    /// What a kind of event states about the run, told apart from the
    /// envelope every event carries: `a_kind_that_moves_no_state_says_so_by_name`
    /// pairs this with each kind's own `is_audit`.
    pub fn derives_nothing(&self) -> bool {
        let heard_from_only = self.nodes.iter().all(|(_, record)| {
            *record
                == yunta_core::events::NodeRecord {
                    last_event_at: record.last_event_at,
                    ..Default::default()
                }
        });
        let rest = RunState {
            nodes: NodeLedger::default(),
            ..self.clone()
        };
        heard_from_only && rest == RunState::default()
    }

    /// A resolution recorded while the run was parked and nothing has
    /// consumed: what a `resolve_gate` call seeded onto the log for this
    /// wake to act on.
    ///
    /// Three ledgers answer this together, which is why it lives here.
    /// The gate ledger holds the resolution and whether an escalation
    /// stands after it; the node ledger holds the terminals and
    /// re-routes that mean the run already acted on it; the run ledger
    /// holds the pause that ends the window it was seeded into. A
    /// decision is pre-seeded only while none of those came after it.
    pub fn pre_seeded(&self, node: &NodeId) -> Option<&GateResolvedPayload> {
        let record = self.gates.get(node)?;
        let (resolution, at) = record.resolved.last()?;
        let consumed = self.nodes.get(node).into_iter().flat_map(|node| {
            [
                node.last_terminal,
                node.last_reroute.as_ref().map(|r| r.seq),
            ]
        });
        let blocker = consumed.flatten().chain(self.run.last_paused_at()).max();
        let standing = record.waiting.as_ref().map(|(_, seq)| *seq);
        (Some(*at) > blocker && Some(*at) > standing).then_some(resolution)
    }

    /// Folds one event into every ledger its domain reaches.
    ///
    /// The dispatch names every domain: a kind this binary knows and no
    /// ledger reads would be a kind that silently derives nothing, which
    /// is the failure this shape exists to make impossible.
    pub fn apply(&mut self, event: &StoredEvent) -> Result<(), ReplayError> {
        let Some(payload) = event.payload() else {
            // A kind this binary does not know: counted, named, never a
            // reason to stop deriving what it does know.
            self.unknown_kinds
                .push((event.seq, event.body.kind_name().to_string()));
            return Ok(());
        };
        let meta = EventMeta::of(event);
        match payload {
            EventPayload::Run(e) => self.run.apply(e, &meta),
            EventPayload::Node(e) => {
                self.node_lifecycle(e, event)?;
                self.nodes.apply(e, &meta);
                self.tasks.apply_criteria(e);
            }
            EventPayload::Session(e) => {
                self.nodes.apply_session(e, &meta);
                self.degradations.apply(e, &meta);
            }
            EventPayload::Tasks(e) => {
                self.tasks.apply(e, &meta).map_err(|UnknownTask(task)| {
                    ReplayError::StatusWithoutTask {
                        seq: event.seq,
                        task,
                    }
                })?;
            }
            EventPayload::Scope(e) => self.grants.apply(e, &meta),
            EventPayload::Findings(e) => self.findings.apply(event.node_id.as_ref(), e),
            EventPayload::Artifacts(e) => {
                self.artifacts.apply(event.node_id.as_ref(), event.seq, e);
            }
            EventPayload::Gates(e) => {
                self.gate_lifecycle(e, event)?;
                self.gates.apply(e, &meta);
            }
            EventPayload::Children(e) => self.children.apply(e, &meta),
        }
        Ok(())
    }

    /// The part of a node's lifecycle that is a rule about the log
    /// rather than a fold of it: a terminal with no start behind it is a
    /// log that stopped making sense.
    fn node_lifecycle(
        &mut self,
        event: &NodeEvent,
        stored: &StoredEvent,
    ) -> Result<(), ReplayError> {
        let running = |state: Option<&NodeState>| matches!(state, Some(NodeState::Running { .. }));
        match event {
            NodeEvent::Finished(_) => {
                let node = require_node_id(stored)?;
                if !running(self.nodes.state(&node)) {
                    return Err(ReplayError::FinishedWithoutStart {
                        seq: stored.seq,
                        node,
                    });
                }
            }
            NodeEvent::Failed(_) => {
                let node = require_node_id(stored)?;
                if !running(self.nodes.state(&node)) {
                    return Err(ReplayError::FailedWithoutStart {
                        seq: stored.seq,
                        node,
                    });
                }
            }
            NodeEvent::Started(_) => {
                require_node_id(stored)?;
            }
            // Audit around the node, and the runner it resolved to:
            // neither is a transition, so neither can be out of order.
            NodeEvent::Rerouted(_)
            | NodeEvent::RunnerResolved(_)
            | NodeEvent::HookExecuted(_)
            | NodeEvent::ContextAssembled(_)
            | NodeEvent::CriteriaChecked(_)
            | NodeEvent::ScopeChecked(_) => {}
        }
        Ok(())
    }

    /// What a gate does to the node it parks. A gate and a round of
    /// questions both suspend a node and restore it, and which state it
    /// comes back to is the node ledger's to hold — so the transition
    /// lives here, where both ledgers are in reach.
    fn gate_lifecycle(
        &mut self,
        event: &GateEvent,
        stored: &StoredEvent,
    ) -> Result<(), ReplayError> {
        // No node = a run-level escalation (the token budget check): it
        // gates the whole invocation, not any node's state.
        let Some(node) = stored.node_id.clone() else {
            return Ok(());
        };
        match event {
            GateEvent::Waiting(p) => {
                self.pre_gate
                    .insert(node.clone(), self.nodes.state(&node).cloned());
                self.nodes.set_state(
                    &node,
                    Some(NodeState::Waiting {
                        external_ref: p.external_ref().map(str::to_string),
                    }),
                );
            }
            GateEvent::Resolved(_) => {
                // Only restores while still `Waiting`: an external
                // gate's poll resolution emits `node_started` *before*
                // `gate_resolved`, so by then the node is already
                // `Running` and the outcome events own its state.
                if matches!(self.nodes.state(&node), Some(NodeState::Waiting { .. })) {
                    let prior = self.pre_gate.remove(&node).flatten();
                    self.nodes.set_state(&node, prior);
                }
            }
            GateEvent::QuestionsAsked(p) => {
                if !matches!(self.nodes.state(&node), Some(NodeState::Running { .. })) {
                    return Err(ReplayError::AskedWithoutStart {
                        seq: stored.seq,
                        node,
                    });
                }
                // The attempt's accounting closes here: the session that
                // asked is done, and no reader counts it again while the
                // node waits.
                self.nodes.add_closed_tokens(&node, p.tokens_used);
                self.pre_gate
                    .insert(node.clone(), self.nodes.state(&node).cloned());
                self.nodes
                    .set_state(&node, Some(NodeState::Waiting { external_ref: None }));
            }
            GateEvent::QuestionsAnswered(_) => {
                if !matches!(self.nodes.state(&node), Some(NodeState::Waiting { .. })) {
                    return Err(ReplayError::AnsweredWithoutAsk {
                        seq: stored.seq,
                        node,
                    });
                }
                // The answer reopens the node exactly where asking left
                // it: `Running`, owed the terminal its close deferred.
                let prior = self.pre_gate.remove(&node).flatten();
                self.nodes.set_state(&node, prior);
            }
        }
        Ok(())
    }
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
    for event in events {
        if let Err(error) = state.apply(event) {
            state.broken = Some(error.to_string());
            break;
        }
    }
    state
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
pub enum ReplayError {
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

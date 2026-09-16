//! Every node's lifecycle, folded once.
//!
//! Nineteen modules used to read the `node_started`/`node_finished`/
//! `node_failed` triple, each with its own rule for what an attempt is,
//! when a session is open and which failure is the current one. Every
//! surface that counts attempts, reports a node's state, times it or
//! lists what it ran reads that here rather than folding the triple
//! itself — with three kinds per attempt, a second fold is a second
//! answer.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::events::meta::EventMeta;
use crate::events::node::kinds::NodeEvent;
use crate::events::node::payloads::{DiscardedCandidate, NodeReroutedPayload, RerouteOrigin};
use crate::events::{Failure, TokenUsage};
use crate::ids::{NodeId, QuestionId, RunnerName, Seq};
use crate::{NonEmpty, RunnerCandidate, TreeId};

/// One node's derived lifecycle state. An enum, not booleans: there is
/// no combination of flags to get wrong.
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
    /// Waiting on a human, and on what.
    Waiting {
        on: NodeWait,
    },
}

/// What a waiting node waits on.
///
/// A gate's kind — internal or external — is a property of the
/// declaration, never of the wait: the scheduler reads it from the
/// workflow, and the state says only whether a handle was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeWait {
    /// A published, unresolved gate (`gate_waiting` with no
    /// `gate_resolved` after it). `external_ref` is the forge's handle
    /// (a PR URL) once one is recorded, `None` for an internal gate.
    Gate { external_ref: Option<String> },
    /// The questions the node asked and nobody answered
    /// (`questions_asked` with no `questions_answered` after it).
    Questions { asked: NonEmpty<QuestionId> },
}

impl NodeState {
    /// What this node waits on, for a reader that has a state and wants
    /// the wait without matching the whole enum.
    pub fn waiting_on(&self) -> Option<&NodeWait> {
        match self {
            NodeState::Waiting { on } => Some(on),
            _ => None,
        }
    }
}

/// A re-route as `node_rerouted` recorded it, on the node it left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reroute {
    pub to: NodeId,
    pub cause: String,
    /// The retry count and the cap it counts against — both `None` for a
    /// gate's routing choice, which is a decision, not a retry.
    pub attempt: Option<u32>,
    pub max: Option<u32>,
    pub origin: RerouteOrigin,
    pub at: DateTime<Utc>,
    pub seq: Seq,
}

impl Reroute {
    /// The re-route `payload` records, placed by its envelope.
    pub fn of(payload: &NodeReroutedPayload, meta: &EventMeta<'_>) -> Self {
        Reroute {
            to: payload.to_node.clone(),
            cause: payload.cause.clone(),
            attempt: payload.attempt,
            max: payload.max_reroutes,
            origin: payload.origin,
            at: meta.at,
            seq: meta.seq,
        }
    }
}

/// The outcome of resolving one runner: the winning candidate plus every
/// candidate passed over, with reasons — exactly what `runner_resolved`
/// records.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRunner {
    pub runner: RunnerName,
    pub chosen: RunnerCandidate,
    pub discarded: Vec<DiscardedCandidate>,
}

/// What the log says about one node.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeRecord {
    /// How many times this node started. The attempt a reader reports is
    /// this, never a count re-derived from somewhere else.
    pub attempts: u32,
    pub state: Option<NodeState>,
    /// Where and when the open attempt started; `None` once it closed.
    pub open_since: Option<(Seq, DateTime<Utc>)>,
    /// The tree the open attempt started from — what its diff is judged
    /// against. `None` for an attempt that recorded none, which is what
    /// a log written before the audit had a starting point carries, and
    /// what a reader takes as the run's own base.
    pub from_tree: Option<TreeId>,
    pub last_terminal: Option<Seq>,
    pub last_failed: Option<Seq>,
    pub last_finished: Option<Seq>,
    pub reroutes: u32,
    pub last_reroute: Option<Reroute>,
    pub runner: Option<ResolvedRunner>,
    /// What every closed attempt of this node spent.
    pub tokens_closed: TokenUsage,
    /// What the attempt now open has spent that is already accounted
    /// for — a round of questions closes its session's accounting when
    /// it asks, so the terminal that follows carries the attempt's whole
    /// spend without counting that session twice.
    pub tokens_this_attempt: TokenUsage,
    /// What the open attempt has reported so far, in `agent_message`
    /// usage — not yet in `tokens_closed`, which only a terminal moves.
    pub tokens_in_flight: TokenUsage,
    /// The sessions the open attempt has opened, oldest first. A node's
    /// attempt is what bounds a session's life: the schema has no
    /// per-session close event, so a session is open exactly while the
    /// attempt that opened it has not reached a terminal.
    pub sessions: Vec<OpenSession>,
    /// Every write this node's sessions had refused, oldest first.
    pub refused: Vec<RefusedWrite>,
    /// The tool calls the open attempt made, oldest first.
    pub calls: Vec<ToolCall>,
    pub last_event_at: Option<DateTime<Utc>>,
    /// The session of a previous attempt that no terminal ever closed —
    /// what a resume finds when a crash cut the attempt between its
    /// start and its verdict. `None` once a later attempt closes
    /// normally, because then nothing was left open.
    pub orphaned_session: Option<OrphanedSession>,
}

/// What a resume finds of the attempt before this one, when that attempt
/// never reached a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrphanedSession {
    /// The session it had open, by the id its adapter gave it.
    Open(crate::ids::SessionId),
    /// It opened none before it was cut.
    NoneRecorded,
}

/// One session a node has open, as `agent_session_opened` recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSession {
    pub session_id: crate::ids::SessionId,
    /// The adapter's own named agent the session runs as; `None` when
    /// the runner named none.
    pub agent: Option<crate::ids::AgentName>,
    /// The model the CLI reported for the session; `None` when it
    /// reported none.
    pub model: Option<crate::ids::ModelName>,
    /// How much of this session the adapter's fence covered; `None`
    /// when it built none.
    pub fence: Option<crate::fence::Coverage>,
    pub opened_at: DateTime<Utc>,
}

/// One write the fence refused: which session tried it, and what it
/// would have touched.
#[derive(Debug, Clone, PartialEq)]
pub struct RefusedWrite {
    pub session_id: crate::ids::SessionId,
    pub target: crate::events::ToolTarget,
}

/// One tool call, as `agent_message` recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// The tool the adapter named; `None` when it reported a call under
    /// no name.
    pub tool_name: Option<String>,
    /// What the call acted on, as the log carries it; `None` when the
    /// adapter reported none.
    pub target: Option<crate::events::ToolTarget>,
    pub at: DateTime<Utc>,
}

/// Every node's record, by id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeLedger {
    per_node: BTreeMap<NodeId, NodeRecord>,
}

impl NodeLedger {
    /// What the log says about `node`; `None` for a node it never names.
    pub fn get<Q>(&self, node: &Q) -> Option<&NodeRecord>
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.get(node)
    }

    /// The tree `node`'s open attempt started from, as its own
    /// `node_started` recorded it — what its diff is judged against.
    /// `None` for a node that never started, and for one whose start
    /// named no tree.
    pub fn from_tree<Q>(&self, node: &Q) -> Option<&TreeId>
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.get(node).and_then(|r| r.from_tree.as_ref())
    }

    /// `node`'s derived state; `None` for a node with none yet.
    pub fn state<Q>(&self, node: &Q) -> Option<&NodeState>
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.get(node).and_then(|r| r.state.as_ref())
    }

    /// Every node the log names, with its record, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &NodeRecord)> {
        self.per_node.iter()
    }

    /// Every record, in node-id order — what a reader that asks about
    /// the run rather than one node walks.
    pub fn values(&self) -> impl Iterator<Item = &NodeRecord> {
        self.per_node.values()
    }

    /// Every record, in node-id order.
    pub fn records(&self) -> impl Iterator<Item = &NodeRecord> {
        self.per_node.values()
    }

    /// Whether the log names no node at all.
    pub fn is_empty(&self) -> bool {
        self.per_node.is_empty()
    }

    /// How many nodes the log names.
    pub fn len(&self) -> usize {
        self.per_node.len()
    }

    /// Whether `node` has a derived state: it started, closed, failed
    /// or is waiting. A node the log only mentions — a re-route target
    /// named by the node that left, a runner resolved ahead of time —
    /// has a record and no state, and has not run.
    pub fn has_state<Q>(&self, node: &Q) -> bool
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node
            .get(node)
            .is_some_and(|record| record.state.is_some())
    }

    /// Whether the log names `node` at all.
    pub fn contains<Q>(&self, node: &Q) -> bool
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.contains_key(node)
    }

    /// Sets `node`'s state — for the pairs a node's own kinds do not
    /// close: a gate parks a node and its resolution restores it, and
    /// the gate ledger is what knows which.
    pub fn set_state(&mut self, node: &NodeId, state: Option<NodeState>) {
        self.per_node.entry(node.clone()).or_default().state = state;
    }

    /// Records what a round of questions closed before the node's own
    /// terminal did: it counts toward the run's total now, and toward
    /// the attempt's own spend when the terminal arrives.
    pub fn add_closed_tokens(&mut self, node: &NodeId, tokens: TokenUsage) {
        let record = self.per_node.entry(node.clone()).or_default();
        record.tokens_closed += tokens;
        record.tokens_this_attempt += tokens;
    }

    /// Folds one node-domain event.
    pub fn apply(&mut self, event: &NodeEvent, meta: &EventMeta<'_>) {
        let Some(node) = meta.node else {
            // A node event with no node on its envelope states nothing
            // about any node. The replay that dispatches here refuses it
            // before this point for the kinds where it is a broken log;
            // for the rest there is simply nothing to record.
            return;
        };
        let record = self.per_node.entry(node.clone()).or_default();
        record.last_event_at = Some(meta.at);
        match event {
            NodeEvent::Started(p) => {
                record.from_tree = p.from_tree.clone();
                // A start while the previous attempt is still open means
                // nothing closed it: whatever session it had is the
                // orphan a resume has to deal with.
                record.orphaned_session =
                    record
                        .open_since
                        .is_some()
                        .then(|| match record.sessions.last() {
                            Some(session) => OrphanedSession::Open(session.session_id.clone()),
                            None => OrphanedSession::NoneRecorded,
                        });
                record.attempts += 1;
                record.state = Some(NodeState::Running { attempt: p.attempt });
                record.open_since = Some((meta.seq, meta.at));
                record.tokens_in_flight = TokenUsage::default();
                // A fresh attempt owes nothing for an earlier round's
                // answer: whatever it produces closes it.
                record.tokens_this_attempt = TokenUsage::default();
                record.sessions.clear();
                record.calls.clear();
            }
            NodeEvent::Finished(p) => {
                let attempt = record.tokens_this_attempt + p.tokens_used;
                record.tokens_closed += p.tokens_used;
                record.tokens_this_attempt = TokenUsage::default();
                record.tokens_in_flight = TokenUsage::default();
                record.open_since = None;
                record.last_terminal = Some(meta.seq);
                record.last_finished = Some(meta.seq);
                record.sessions.clear();
                record.calls.clear();
                record.state = Some(NodeState::Finished {
                    outcome: p.outcome.clone(),
                    tokens: attempt,
                });
            }
            NodeEvent::Failed(p) => {
                let attempt = record.tokens_this_attempt + p.tokens_used;
                record.tokens_closed += p.tokens_used;
                record.tokens_this_attempt = TokenUsage::default();
                record.tokens_in_flight = TokenUsage::default();
                record.open_since = None;
                record.last_terminal = Some(meta.seq);
                record.last_failed = Some(meta.seq);
                record.sessions.clear();
                record.calls.clear();
                record.state = Some(NodeState::Failed {
                    failure: p.failure.clone(),
                    tokens: attempt,
                    retryable: p.retryable,
                });
            }
            NodeEvent::Rerouted(p) => {
                record.reroutes += 1;
                record.last_reroute = Some(Reroute::of(p, meta));
            }
            NodeEvent::RunnerResolved(p) => {
                record.runner = Some(ResolvedRunner {
                    runner: p.runner.clone(),
                    chosen: p.chosen.clone(),
                    discarded: p.discarded.clone(),
                });
            }
            // Audit: the engine ran a mechanical check around the node
            // and recorded what it found. None of it moves the node's
            // lifecycle — what a criterion or a scope diff decided
            // reaches state through the terminal the node then gets.
            NodeEvent::HookExecuted(_)
            | NodeEvent::ContextAssembled(_)
            | NodeEvent::CriteriaChecked(_)
            | NodeEvent::ScopeChecked(_) => {}
        }
    }

    /// Folds a session-domain event onto the node that wrote it: a
    /// session belongs to the attempt that opened it, so what is open
    /// and what it has spent is part of that node's record.
    pub fn apply_session(
        &mut self,
        event: &crate::events::session::kinds::SessionEvent,
        meta: &EventMeta<'_>,
    ) {
        use crate::events::session::kinds::SessionEvent;
        use crate::events::AgentMessageType;

        let Some(node) = meta.node else {
            return;
        };
        let record = self.per_node.entry(node.clone()).or_default();
        record.last_event_at = Some(meta.at);
        match event {
            SessionEvent::Opened(p) => record.sessions.push(OpenSession {
                session_id: p.session_id.clone(),
                agent: p.agent.clone(),
                model: p.model.clone(),
                fence: p.fence.clone(),
                opened_at: meta.at,
            }),
            SessionEvent::WriteRefused(p) => record.refused.push(RefusedWrite {
                session_id: p.session_id.clone(),
                target: p.target.clone(),
            }),
            SessionEvent::Message(p) => match p.message_type {
                AgentMessageType::ToolUse => record.calls.push(ToolCall {
                    tool_name: p.tool_name.clone(),
                    target: p.target.clone(),
                    at: meta.at,
                }),
                AgentMessageType::Usage => {
                    record.tokens_in_flight += TokenUsage {
                        input: p.input_tokens.unwrap_or_default(),
                        output: p.output_tokens.unwrap_or_default(),
                        cached: p.cached_input_tokens,
                    }
                }
                // What the agent said about itself, for a reader. It
                // moves nothing.
                AgentMessageType::Note => {}
            },
            // A degradation is the adapter's, not the attempt's: the
            // session ledger holds it.
            SessionEvent::CapabilityDegraded(_) => {}
        }
    }
}

//! The events a log cannot take, the error each is refused with, and what a
//! gate does to the node it parks.
//!
//! Replay reads a refusal before any ledger moves. A refused event moves no
//! ledger and books no time, and replay stops at it with the state up to
//! that point. Once an event is known to follow the log, the gate transition
//! is the one owner step that reaches two ledgers at once, so it lives here
//! beside the gate refusals it follows.

use chrono::{DateTime, Utc};
use yunta_core::events::{
    EventMeta, EventPayload, GateEvent, NodeEvent, NodeWait, ScopeEvent, StoredEvent, TaskEvent,
};
use yunta_core::{NodeId, NonEmpty, Seq, TaskId};

use super::{NodeState, RunState};

impl RunState {
    /// Whether `event` can follow what this state holds. A kind this binary
    /// does not know is never refused: it is counted instead.
    pub(super) fn refuse(&self, event: &StoredEvent) -> Result<(), ReplayError> {
        let Some(payload) = event.payload() else {
            return Ok(());
        };
        match payload {
            EventPayload::Node(e) => self.refuse_node(e, event),
            EventPayload::Session(_) => refuse_session(event),
            EventPayload::Tasks(e) => self.refuse_task(e, event),
            EventPayload::Scope(e) => refuse_scope(e, event),
            EventPayload::Gates(e) => self.refuse_gate(e, event),
            EventPayload::Run(_)
            | EventPayload::Findings(_)
            | EventPayload::Artifacts(_)
            | EventPayload::Children(_) => Ok(()),
        }
    }

    /// A terminal needs the start it closes, and every node fact names the
    /// node it is about.
    fn refuse_node(&self, event: &NodeEvent, stored: &StoredEvent) -> Result<(), ReplayError> {
        let running =
            |node: &NodeId| matches!(self.nodes.state(node), Some(NodeState::Running { .. }));
        match event {
            NodeEvent::Finished(_) => {
                let node = require_node_id(stored)?;
                if !running(node) {
                    return Err(ReplayError::FinishedWithoutStart {
                        seq: stored.seq,
                        node: node.clone(),
                    });
                }
            }
            NodeEvent::Failed(_) => {
                let node = require_node_id(stored)?;
                if !running(node) {
                    return Err(ReplayError::FailedWithoutStart {
                        seq: stored.seq,
                        node: node.clone(),
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
            | NodeEvent::ScopeChecked(_)
            | NodeEvent::PullRequestOpened(_) => {}
        }
        Ok(())
    }

    /// A question is asked by a running node and names at least one
    /// question, and an answer comes back to a node that is waiting. A
    /// run-level gate names no node and is never refused.
    fn refuse_gate(&self, event: &GateEvent, stored: &StoredEvent) -> Result<(), ReplayError> {
        let Some(node) = &stored.node_id else {
            return Ok(());
        };
        match event {
            GateEvent::QuestionsAsked(p) => {
                if !matches!(self.nodes.state(node), Some(NodeState::Running { .. })) {
                    return Err(ReplayError::AskedWithoutStart {
                        seq: stored.seq,
                        node: node.clone(),
                    });
                }
                if p.questions.is_empty() {
                    return Err(ReplayError::AskedNothing {
                        seq: stored.seq,
                        node: node.clone(),
                    });
                }
            }
            GateEvent::QuestionsAnswered(_) => {
                if !matches!(self.nodes.state(node), Some(NodeState::Waiting { .. })) {
                    return Err(ReplayError::AnsweredWithoutAsk {
                        seq: stored.seq,
                        node: node.clone(),
                    });
                }
            }
            GateEvent::Waiting(_) | GateEvent::Resolved(_) | GateEvent::AskingOpened(_) => {}
        }
        Ok(())
    }

    /// A status change, a deviation or a declared criterion names a task the
    /// log registered.
    fn refuse_task(&self, event: &TaskEvent, stored: &StoredEvent) -> Result<(), ReplayError> {
        let task = match event {
            TaskEvent::StatusChanged(p) => &p.task_id,
            TaskEvent::DeviationDeclared(p) => &p.task_id,
            TaskEvent::DeviationResolved(p) => &p.task_id,
            TaskEvent::CriterionDeclaredUnpassable(p) => &p.task_id,
            TaskEvent::Registered(_) | TaskEvent::CheckStarted(_) | TaskEvent::CheckAnswered(_) => {
                return Ok(());
            }
        };
        if self.tasks.contains(task) {
            Ok(())
        } else {
            Err(ReplayError::StatusWithoutTask {
                seq: stored.seq,
                task: task.clone(),
            })
        }
    }

    /// What a gate does to the node it parks. A gate and a round of
    /// questions both suspend a node and restore it, and which state it
    /// comes back to is the node ledger's to hold — so the transition
    /// lives here, where both ledgers are in reach.
    pub(super) fn gate_transition(
        &mut self,
        event: &GateEvent,
        stored: &StoredEvent,
        meta: &EventMeta<'_>,
    ) -> Result<(), ReplayError> {
        // No node = a run-level escalation (the token budget check): it
        // gates the whole invocation, not any node's state.
        let Some(node) = stored.node_id.clone() else {
            return Ok(());
        };
        match event {
            GateEvent::Waiting(p) => {
                let external_ref = p.external_ref().map(str::to_string);
                self.park(&node, NodeWait::Gate { external_ref });
                self.nodes.stop_working(&node, meta, self.run.suspensions());
            }
            GateEvent::Resolved(_) => {
                // Only restores while still `Waiting`: an external
                // gate's poll resolution emits `node_started` *before*
                // `gate_resolved`, so by then the node is already
                // `Running` and the outcome events own its state.
                if matches!(self.nodes.state(&node), Some(NodeState::Waiting { .. })) {
                    self.unpark(&node, stored.timestamp);
                }
            }
            GateEvent::QuestionsAsked(p) => {
                // The refusal reads the questions first, so a log that
                // reaches here names at least one; a round with none
                // would be no wait at all.
                let Some(asked) = NonEmpty::new(p.questions.clone()) else {
                    return Err(ReplayError::AskedNothing {
                        seq: stored.seq,
                        node,
                    });
                };
                // The attempt's accounting closes here: the session that
                // asked is done, and no reader counts it again while the
                // node waits.
                self.nodes
                    .settle_asking(&node, p.tokens_used, meta, self.run.suspensions());
                self.park(&node, NodeWait::Questions { asked });
            }
            // The answer reopens the node exactly where asking left it:
            // `Running`, owed the terminal its close deferred.
            GateEvent::QuestionsAnswered(_) => self.unpark(&node, stored.timestamp),
            // A person is asked about the node, so it stops working: the
            // escalation and its answer are written together once the
            // person answers, and the answer resumes the work.
            GateEvent::AskingOpened(_) => {
                self.nodes.stop_working(&node, meta, self.run.suspensions());
            }
        }
        Ok(())
    }

    /// Parks `node` on `on`, keeping the state it had so the close of the
    /// wait can restore it.
    fn park(&mut self, node: &NodeId, on: NodeWait) {
        self.pre_gate
            .insert(node.clone(), self.nodes.state(node).cloned());
        self.nodes.set_state(node, Some(NodeState::Waiting { on }));
    }

    /// Restores the state `node` had before its wait, and resumes the work
    /// the wait stopped.
    fn unpark(&mut self, node: &NodeId, at: DateTime<Utc>) {
        let prior = self.pre_gate.remove(node).flatten();
        self.nodes.set_state(node, prior);
        self.nodes.resume_working(node, at);
    }
}

/// A grant, a denial or a withdrawal acts on a task, or on the node it is
/// written under: one that names neither has no subject to act on.
fn refuse_scope(event: &ScopeEvent, stored: &StoredEvent) -> Result<(), ReplayError> {
    let names_task = match event {
        ScopeEvent::Granted(p) => p.task_id.is_some(),
        ScopeEvent::Denied(p) => p.task_id.is_some(),
        ScopeEvent::Requested(_) | ScopeEvent::Derived(_) | ScopeEvent::Withdrawn(_) => true,
    };
    if names_task || stored.node_id.is_some() {
        Ok(())
    } else {
        Err(ReplayError::UngrantableSubject { seq: stored.seq })
    }
}

/// A session fact belongs to the attempt its session ran in, so one with no
/// node has nothing to be read against.
fn refuse_session(stored: &StoredEvent) -> Result<(), ReplayError> {
    if stored.node_id.is_some() {
        Ok(())
    } else {
        Err(ReplayError::SessionWithoutNode {
            seq: stored.seq,
            kind: stored.body.kind_name().to_string(),
        })
    }
}

/// The node `event` names; a node fact that names none is a broken log.
fn require_node_id(event: &StoredEvent) -> Result<&NodeId, ReplayError> {
    event
        .node_id
        .as_ref()
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
    #[error("seq {seq}: node `{node}` got questions_asked naming no question to answer")]
    AskedNothing { seq: Seq, node: NodeId },
    #[error("seq {seq}: task `{task}` got task_status_changed without a prior task_registered")]
    StatusWithoutTask { seq: Seq, task: TaskId },
    #[error("seq {seq}: a grant names neither a task nor a node")]
    UngrantableSubject { seq: Seq },
    #[error("seq {seq}: `{kind}` is a session fact with no node")]
    SessionWithoutNode { seq: Seq, kind: String },
}

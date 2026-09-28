//! What a run granted beyond the scope a task or a node declared, folded
//! once.
//!
//! Three places used to read the three expansion kinds with their own
//! rule: one for the paths an attempt may write, one to tell whether
//! anything was ever granted, one to summarize the run. Every surface
//! that asks what a task or a node may reach reads it here.

use std::collections::BTreeMap;

use crate::events::meta::EventMeta;
use crate::events::scope::kinds::ScopeEvent;
use crate::glob::ScopeGlob;
use crate::ids::{NodeId, Seq, TaskId};

/// Every grant this run made, by task and by node.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrantLedger {
    per_task: BTreeMap<TaskId, Vec<ScopeGlob>>,
    /// The last answer each task's request got.
    answers: BTreeMap<TaskId, ScopeAnswer>,
    per_node: BTreeMap<NodeId, NodeGrants>,
    granted: u32,
    denied: u32,
    requested: u32,
}

/// What a node was granted, and where the last grant stands on the log —
/// what tells a decision to widen it already acted on from one still
/// owed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeGrants {
    paths: Vec<ScopeGlob>,
    last_at: Seq,
    /// What the latest grant added.
    last_paths: Vec<ScopeGlob>,
}

/// What a scope request was answered with, as the log states it — what
/// a session that asked is told when it picks its work back up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeAnswer {
    /// Granted these paths.
    Granted(Vec<ScopeGlob>),
    /// Refused, with the reason the decider gave, when it gave one.
    Denied(Option<String>),
}

impl GrantLedger {
    /// The paths granted to `task`, in grant order — what a later
    /// attempt's effective scope adds to what the task declared.
    pub fn paths_for(&self, task: &TaskId) -> &[ScopeGlob] {
        self.per_task.get(task).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The paths granted to `node`'s own scope, in grant order — what
    /// its later attempts are fenced to and audited against beside what
    /// it declared.
    pub fn paths_for_node(&self, node: &NodeId) -> &[ScopeGlob] {
        self.per_node
            .get(node)
            .map(|grants| grants.paths.as_slice())
            .unwrap_or(&[])
    }

    /// The last answer `task`'s scope request got.
    pub fn answer_for(&self, task: &TaskId) -> Option<&ScopeAnswer> {
        self.answers.get(task)
    }

    /// What the latest grant to `node`'s own scope added.
    pub fn last_granted_to_node_paths(&self, node: &NodeId) -> &[ScopeGlob] {
        self.per_node
            .get(node)
            .map(|grants| grants.last_paths.as_slice())
            .unwrap_or(&[])
    }

    /// Where the latest grant to `node`'s own scope stands on the log.
    pub fn last_granted_to_node(&self, node: &NodeId) -> Option<Seq> {
        self.per_node.get(node).map(|grants| grants.last_at)
    }

    /// How many grants this run made — the count `max_per_run` is
    /// checked against.
    pub fn granted(&self) -> u32 {
        self.granted
    }

    /// How many requests were refused.
    pub fn denied(&self) -> u32 {
        self.denied
    }

    /// How many expansions were asked for at all.
    pub fn requested(&self) -> u32 {
        self.requested
    }

    /// Folds one scope-domain event. A grant that names no task widens
    /// the node it is written under; one that names neither widens
    /// nothing, and is still counted.
    pub fn apply(&mut self, event: &ScopeEvent, meta: &EventMeta<'_>) {
        match event {
            ScopeEvent::Requested(_) => self.requested += 1,
            ScopeEvent::Granted(p) => {
                self.granted += 1;
                match (&p.task_id, meta.node) {
                    (Some(task), _) => {
                        self.per_task
                            .entry(task.clone())
                            .or_default()
                            .extend(p.paths.iter().cloned());
                        self.answers
                            .insert(task.clone(), ScopeAnswer::Granted(p.paths.clone()));
                    }
                    (None, Some(node)) => {
                        let grants = self.per_node.entry(node.clone()).or_insert(NodeGrants {
                            paths: Vec::new(),
                            last_at: meta.seq,
                            last_paths: Vec::new(),
                        });
                        grants.paths.extend(p.paths.iter().cloned());
                        grants.last_at = meta.seq;
                        grants.last_paths = p.paths.clone();
                    }
                    (None, None) => {}
                }
            }
            ScopeEvent::Denied(p) => {
                self.denied += 1;
                self.answers.insert(
                    p.task_id.clone(),
                    ScopeAnswer::Denied(p.denial_reason.clone()),
                );
            }
        }
    }
}

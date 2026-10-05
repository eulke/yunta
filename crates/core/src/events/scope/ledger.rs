//! What a run granted beyond the scope a task or a node declared, and what
//! it derived a task may reach beyond it, folded once.
//!
//! Three places used to read the three expansion kinds with their own
//! rule: one for the paths an attempt may write, one to tell whether
//! anything was ever granted, one to summarize the run. Every surface
//! that asks what a task or a node may reach reads it here.

use std::collections::BTreeMap;

use crate::events::meta::EventMeta;
use crate::events::scope::kinds::ScopeEvent;
use crate::events::scope::payloads::ScopeDerivedPayload;
use crate::glob::ScopeGlob;
use crate::ids::{NodeId, Seq, TaskId};

/// Every grant this run made, by task and by node.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrantLedger {
    per_task: BTreeMap<TaskId, Vec<ScopeGlob>>,
    /// The last answer each task's request got.
    answers: BTreeMap<TaskId, ScopeAnswer>,
    per_node: BTreeMap<NodeId, NodeGrants>,
    /// The latest derivation stated for each task — each one replaces the
    /// one before, so a recut that changed its shapes changes its reach.
    derived: BTreeMap<TaskId, ScopeDerivedPayload>,
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
    /// Everything `task` may reach beyond what it declared: the files that
    /// name a shape it owns, then every path granted to it, in grant
    /// order. What its fence, its tools, the audit of its close and the
    /// re-verification after a rebase add to its declared scope.
    pub fn reach_for(&self, task: &TaskId) -> Vec<ScopeGlob> {
        let derived = self.derived.get(task).map(|d| d.paths.as_slice());
        let granted = self.per_task.get(task).map(Vec::as_slice);
        derived
            .unwrap_or(&[])
            .iter()
            .chain(granted.unwrap_or(&[]))
            .cloned()
            .collect()
    }

    /// The latest derivation the log states for `task`.
    pub fn derived_for(&self, task: &TaskId) -> Option<&ScopeDerivedPayload> {
        self.derived.get(task)
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
    /// checked against. A derived reach is no grant, and is not counted.
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
    /// nothing, and is still counted. A derivation replaces the task's
    /// last one.
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
            ScopeEvent::Derived(p) => {
                self.derived.insert(p.task_id.clone(), p.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ScopeExpansionGrantedPayload;
    use crate::policy::ScopeExpansionMode;

    fn fold(ledger: &mut GrantLedger, seq: u64, event: ScopeEvent) {
        let meta = EventMeta {
            seq: Seq::from(seq),
            at: chrono::DateTime::UNIX_EPOCH,
            node: None,
        };
        ledger.apply(&event, &meta);
    }

    fn derived(paths: &[&str]) -> ScopeEvent {
        ScopeEvent::Derived(ScopeDerivedPayload {
            task_id: TaskId::from_static("task-h"),
            paths: paths.iter().map(|path| ScopeGlob::from(*path)).collect(),
            shapes: vec!["build".to_string()],
            common: Vec::new(),
            at: "a".repeat(40).parse().unwrap(),
        })
    }

    #[test]
    fn rederivation_after_recut_replaces_reach() {
        let mut ledger = GrantLedger::default();
        fold(&mut ledger, 1, derived(&["caller.rs", "other.rs"]));
        fold(&mut ledger, 2, derived(&["moved.rs"]));

        let task = TaskId::from_static("task-h");
        assert_eq!(ledger.reach_for(&task), [ScopeGlob::from("moved.rs")]);
    }

    /// A derived reach is no grant: it counts toward no cap, answers no
    /// request, and a grant still adds to it.
    #[test]
    fn derived_reach_is_no_grant() {
        let mut ledger = GrantLedger::default();
        fold(&mut ledger, 1, derived(&["caller.rs"]));
        assert_eq!(ledger.granted(), 0);

        fold(
            &mut ledger,
            2,
            ScopeEvent::Granted(ScopeExpansionGrantedPayload {
                task_id: Some(TaskId::from_static("task-h")),
                decided_by: crate::events::Decider::Rule,
                mode: ScopeExpansionMode::Rules,
                count_this_run: 1,
                paths: vec![ScopeGlob::from("b.rs")],
            }),
        );

        let task = TaskId::from_static("task-h");
        let reach = ledger.reach_for(&task);
        assert_eq!(
            reach,
            [ScopeGlob::from("caller.rs"), ScopeGlob::from("b.rs")]
        );
        assert_eq!(ledger.granted(), 1);
    }
}

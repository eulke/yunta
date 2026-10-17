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
use crate::events::scope::payloads::{
    Decider, ScopeDerivedPayload, ScopeExpansionDeniedPayload, ScopeExpansionGrantedPayload,
};
use crate::events::SubjectError;
use crate::expansion::Denial;
use crate::glob::ScopeGlob;
use crate::ids::{NodeId, Seq, TaskId};

/// Every grant this run made, by task and by node.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrantLedger {
    per_task: BTreeMap<TaskId, Vec<ScopeGlob>>,
    /// The paths a person granted each task, a part of `per_task`: what
    /// lifts the protection of those paths for that task alone.
    person_per_task: BTreeMap<TaskId, Vec<ScopeGlob>>,
    /// The last answer each task's request got.
    answers: BTreeMap<TaskId, ScopeAnswer>,
    /// The last answer each node's own request got: a denial that names
    /// no task is answered to the node it is written under.
    node_answers: BTreeMap<NodeId, ScopeAnswer>,
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
    /// Refused, with the typed reason the decider gave, when the log
    /// states one.
    Denied(Option<Denial>),
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

    /// The paths a person granted `task`, in grant order: the only paths
    /// whose protection a grant lifts for that task.
    pub fn person_granted_to_task(&self, task: &TaskId) -> &[ScopeGlob] {
        self.person_per_task
            .get(task)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The last answer `task`'s scope request got.
    pub fn answer_for(&self, task: &TaskId) -> Option<&ScopeAnswer> {
        self.answers.get(task)
    }

    /// The last answer the scope request of `node`'s own scope got.
    pub fn node_answer_for(&self, node: &NodeId) -> Option<&ScopeAnswer> {
        self.node_answers.get(node)
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

    /// How many grants the engine made this run — the count `max_per_run`
    /// is checked against. A person's grant and a derived reach are not
    /// counted.
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
    /// the node it is written under. A grant or a denial that names
    /// neither a task nor a node has no subject to widen or to answer: it
    /// is refused, and counts for nothing. A person's grant widens its
    /// subject but counts toward no cap. A derivation replaces the task's
    /// last one.
    pub fn apply(&mut self, event: &ScopeEvent, meta: &EventMeta<'_>) -> Result<(), SubjectError> {
        match event {
            ScopeEvent::Requested(_) => self.requested += 1,
            ScopeEvent::Granted(p) => {
                self.widen(p, meta)?;
                match &p.decided_by {
                    Decider::Rule | Decider::Evidence { .. } => self.granted += 1,
                    Decider::Person { .. } => {}
                }
            }
            ScopeEvent::Denied(p) => {
                self.answer_denial(p, meta)?;
                self.denied += 1;
            }
            ScopeEvent::Derived(p) => {
                self.derived.insert(p.task_id.clone(), p.clone());
            }
            // A withdrawn request was never decided: it grants and answers nothing.
            ScopeEvent::Withdrawn(_) => {}
        }
        Ok(())
    }

    /// Adds a grant's paths to the task it names, or to the node it is
    /// written under when it names no task.
    fn widen(
        &mut self,
        p: &ScopeExpansionGrantedPayload,
        meta: &EventMeta<'_>,
    ) -> Result<(), SubjectError> {
        match (&p.task_id, meta.node) {
            (Some(task), _) => {
                self.per_task
                    .entry(task.clone())
                    .or_default()
                    .extend(p.paths.iter().cloned());
                if matches!(p.decided_by, Decider::Person { .. }) {
                    self.person_per_task
                        .entry(task.clone())
                        .or_default()
                        .extend(p.paths.iter().cloned());
                }
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
            (None, None) => return Err(SubjectError::UngrantableSubject),
        }
        Ok(())
    }

    /// Answers a denied request for the task it names, or for the node it
    /// is written under when it names no task.
    fn answer_denial(
        &mut self,
        p: &ScopeExpansionDeniedPayload,
        meta: &EventMeta<'_>,
    ) -> Result<(), SubjectError> {
        match (&p.task_id, meta.node) {
            (Some(task), _) => {
                self.answers
                    .insert(task.clone(), ScopeAnswer::Denied(p.why()));
            }
            (None, Some(node)) => {
                self.node_answers
                    .insert(node.clone(), ScopeAnswer::Denied(p.why()));
            }
            (None, None) => return Err(SubjectError::UngrantableSubject),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ScopeExpansionDeniedPayload;
    use crate::events::ScopeExpansionGrantedPayload;
    use crate::policy::ScopeExpansionMode;

    fn fold(ledger: &mut GrantLedger, seq: u64, event: ScopeEvent) {
        let meta = EventMeta {
            seq: Seq::from(seq),
            at: chrono::DateTime::UNIX_EPOCH,
            node: None,
        };
        ledger
            .apply(&event, &meta)
            .expect("the fixture names its subject");
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
                mode: Some(ScopeExpansionMode::Rules),
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

    #[test]
    fn an_old_denial_log_still_answers_with_its_text() {
        let mut ledger = GrantLedger::default();
        let old: ScopeExpansionDeniedPayload = serde_json::from_value(serde_json::json!({
            "task_id": "task-h", "decided_by": {"type": "rule"}, "mode": "rules",
            "count_this_run": 1, "denial_reason": "outside the declared `within` ceiling"}))
        .unwrap();
        fold(&mut ledger, 1, ScopeEvent::Denied(old));
        assert_eq!(
            ledger.answer_for(&TaskId::from_static("task-h")),
            Some(&ScopeAnswer::Denied(Some(Denial::Legacy {
                text: "outside the declared `within` ceiling".into(),
            })))
        );
    }

    /// A person's grant widens its task and lifts the protection of its
    /// paths for that task alone, and counts toward no cap: an engine
    /// grant still does.
    #[test]
    fn a_person_grant_widens_its_task_and_counts_toward_no_cap() {
        let mut ledger = GrantLedger::default();
        let task = TaskId::from_static("task-h");
        let grant = |decided_by: Decider, path: &str| {
            ScopeEvent::Granted(ScopeExpansionGrantedPayload {
                task_id: Some(TaskId::from_static("task-h")),
                decided_by,
                mode: Some(ScopeExpansionMode::Ask),
                count_this_run: 1,
                paths: vec![ScopeGlob::from(path)],
            })
        };

        fold(
            &mut ledger,
            1,
            grant(Decider::Person { id: "eulke".into() }, "AGENTS.md"),
        );
        assert_eq!(ledger.granted(), 0);

        fold(
            &mut ledger,
            2,
            grant(
                Decider::Evidence {
                    criterion: "tests".to_string(),
                },
                "b.rs",
            ),
        );
        assert_eq!(ledger.granted(), 1);
        assert_eq!(
            ledger.reach_for(&task),
            [ScopeGlob::from("AGENTS.md"), ScopeGlob::from("b.rs")]
        );
        assert_eq!(
            ledger.person_granted_to_task(&task).to_vec(),
            vec![ScopeGlob::from("AGENTS.md")]
        );
    }

    /// A typed denial is written with its reason and no text beside it,
    /// reads back with the same reason, and answers its task with it.
    #[test]
    fn a_typed_denial_is_written_with_its_reason_only() {
        let reason = Denial::Chosen {
            option: "deny".to_string(),
            note: None,
        };
        let denied = ScopeExpansionDeniedPayload {
            task_id: Some(TaskId::from_static("task-h")),
            decided_by: Decider::Rule,
            mode: Some(ScopeExpansionMode::Ask),
            count_this_run: 1,
            why: Some(reason.clone()),
            legacy_text: None,
        };
        let written = serde_json::to_value(&denied).unwrap();
        assert!(written.get("denial_reason").is_none(), "{written}");

        let read: ScopeExpansionDeniedPayload = serde_json::from_value(written).unwrap();
        assert_eq!(read.why(), Some(reason.clone()));

        let mut ledger = GrantLedger::default();
        fold(&mut ledger, 1, ScopeEvent::Denied(denied));
        assert_eq!(
            ledger.answer_for(&TaskId::from_static("task-h")),
            Some(&ScopeAnswer::Denied(Some(reason)))
        );
    }

    /// A grant a node made carries no mode, and writes none.
    #[test]
    fn a_node_grant_without_a_mode_writes_none() {
        let line = serde_json::json!({
            "decided_by": {"type": "rule"},
            "count_this_run": 1,
            "paths": ["Cargo.toml"]
        });
        let grant: ScopeExpansionGrantedPayload = serde_json::from_value(line).unwrap();
        assert_eq!(grant.mode, None);
        let written = serde_json::to_value(&grant).unwrap();
        assert!(written.get("mode").is_none(), "{written}");
    }
}

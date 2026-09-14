//! What a run granted beyond the scope a task declared, folded once.
//!
//! Three places used to read the three expansion kinds with their own
//! rule: one for the paths an attempt may write, one to tell whether
//! anything was ever granted, one to summarize the run. Every surface
//! that asks what a task may reach reads it here.

use std::collections::BTreeMap;

use crate::events::meta::EventMeta;
use crate::events::scope::kinds::ScopeEvent;
use crate::glob::ScopeGlob;
use crate::ids::TaskId;

/// Every grant this run made, by task.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrantLedger {
    per_task: BTreeMap<TaskId, Vec<ScopeGlob>>,
    granted: u32,
    denied: u32,
    requested: u32,
}

impl GrantLedger {
    /// The paths granted to `task`, in grant order — what a later
    /// attempt's effective scope adds to what the task declared.
    pub fn paths_for(&self, task: &TaskId) -> &[ScopeGlob] {
        self.per_task.get(task).map(Vec::as_slice).unwrap_or(&[])
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

    /// Folds one scope-domain event.
    pub fn apply(&mut self, event: &ScopeEvent, _meta: &EventMeta<'_>) {
        match event {
            ScopeEvent::Requested(_) => self.requested += 1,
            ScopeEvent::Granted(p) => {
                self.granted += 1;
                self.per_task
                    .entry(p.task_id.clone())
                    .or_default()
                    .extend(p.paths.iter().cloned());
            }
            ScopeEvent::Denied(_) => self.denied += 1,
        }
    }
}

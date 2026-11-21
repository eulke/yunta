//! The scope side of the node ledger: the checks of a node's own work, kept
//! on its record, and the union over the run of every check's paths, kept
//! on the ledger. A task's check is the tasks' ledger's to keep on the task;
//! it counts in the union all the same.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{NodeLedger, NodeRecord};
use crate::events::node::payloads::ScopeCheckedPayload;

/// Every path the run's scope checks found changed, and those of them
/// outside what the work may change, each once, over every check: a task's
/// and a node's, a superseded attempt's too. Not one check's diff, which a
/// task's record or a node's keeps as the log has it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeUnion {
    pub diff: BTreeSet<PathBuf>,
    pub violations: BTreeSet<PathBuf>,
}

impl ScopeUnion {
    /// A scope check's paths, joined to the ones the run's checks found.
    pub(super) fn take(&mut self, check: &ScopeCheckedPayload) {
        self.diff.extend(check.diff.iter().cloned());
        self.violations.extend(check.violations.iter().cloned());
    }
}

impl NodeRecord {
    /// A `scope_checked` under this node: kept when it checks the node's
    /// own work, which names no task.
    pub(super) fn scope_checked(&mut self, check: &ScopeCheckedPayload) {
        if check.task_id.is_none() {
            self.scope_checks.push(check.clone());
        }
    }
}

impl NodeLedger {
    /// The union of every scope check's paths over the run.
    pub fn scope_union(&self) -> &ScopeUnion {
        &self.scope_union
    }
}

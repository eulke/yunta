//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// The tasks document, as a run works it: a task registered, and a task — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Registered(TaskRegisteredPayload),
    StatusChanged(TaskStatusChangedPayload),
}

impl TaskEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &["task_registered", "task_status_changed"];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Registered(_) => "task_registered",
            Self::StatusChanged(_) => "task_status_changed",
        }
    }

    /// Whether this kind is audit: the log carries it so a reader can
    /// see what the engine did, and no ledger moves when it arrives.
    ///
    /// Every other kind moves state, and its ledger's `apply` names it.
    /// The pair is proved: `a_kind_that_moves_no_state_says_so_by_name`
    /// applies one of each to an empty state and checks that exactly the
    /// kinds answering `false` here change it.
    pub fn is_audit(&self) -> bool {
        match self {
            Self::Registered(_) => false,
            Self::StatusChanged(_) => false,
        }
    }

    /// The shape version of this kind. Every kind starts at 1 and a
    /// version is per kind, never per domain and never global: a kind
    /// that gains an incompatible shape becomes a new name, and only
    /// that one moves.
    pub fn schema_version(&self) -> u32 {
        1
    }
}

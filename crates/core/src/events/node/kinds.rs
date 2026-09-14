//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// One node of the graph: what it resolved to run on, what it started, — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeEvent {
    RunnerResolved(RunnerResolvedPayload),
    BaselineCaptured(BaselineCapturedPayload),
    Started(NodeStartedPayload),
    ContextAssembled(ContextAssembledPayload),
    CriteriaChecked(CriteriaCheckedPayload),
    ScopeChecked(ScopeCheckedPayload),
    Finished(NodeFinishedPayload),
    Failed(NodeFailedPayload),
    HookExecuted(HookExecutedPayload),
    Rerouted(NodeReroutedPayload),
}

impl NodeEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "runner_resolved",
        "baseline_captured",
        "node_started",
        "context_assembled",
        "criteria_checked",
        "scope_checked",
        "node_finished",
        "node_failed",
        "hook_executed",
        "node_rerouted",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::RunnerResolved(_) => "runner_resolved",
            Self::BaselineCaptured(_) => "baseline_captured",
            Self::Started(_) => "node_started",
            Self::ContextAssembled(_) => "context_assembled",
            Self::CriteriaChecked(_) => "criteria_checked",
            Self::ScopeChecked(_) => "scope_checked",
            Self::Finished(_) => "node_finished",
            Self::Failed(_) => "node_failed",
            Self::HookExecuted(_) => "hook_executed",
            Self::Rerouted(_) => "node_rerouted",
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

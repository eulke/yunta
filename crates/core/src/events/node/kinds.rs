//! Which facts this domain records.

use super::payloads::*;

/// One node of the graph: what it resolved to run on, what it started,
/// what it closed or failed with, where it was re-routed, the pull
/// request it opened, and the checks the engine ran around it — one
/// variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeEvent {
    RunnerResolved(RunnerResolvedPayload),
    Started(NodeStartedPayload),
    ContextAssembled(ContextAssembledPayload),
    CriteriaChecked(CriteriaCheckedPayload),
    ScopeChecked(ScopeCheckedPayload),
    Finished(NodeFinishedPayload),
    Failed(NodeFailedPayload),
    HookExecuted(HookExecutedPayload),
    Rerouted(NodeReroutedPayload),
    PullRequestOpened(PullRequestOpenedPayload),
}

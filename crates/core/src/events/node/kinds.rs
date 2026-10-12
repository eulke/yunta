//! Which facts this domain records.

use super::payloads::*;

/// One node of the graph: what it resolved to run on, what it started, — one variant per kind.
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

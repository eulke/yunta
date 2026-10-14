//! Resolving a node's runner into an adapter, model and agent, opening
//! its per-run tools, and recording what the adapter cannot enforce.

use yunta_core::events::{
    CapabilityUse, EventPayload, Failure, RunnerRefusal, RunnerResolvedPayload,
};
use yunta_core::{AdapterId, Node, NodeId, RunAdapter};

use crate::runner::{resolve_runner, RunnerError};

use super::node_close::fail;
use super::step::Step;
use super::{RunCtx, RunError};
use yunta_core::events::NodeEvent;

/// Resolves the node's runner or fails the node; on success emits
/// `runner_resolved` and hands back the request pieces.
pub(super) async fn resolve_node_runner(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Step<yunta_core::RunnerCandidate>, RunError> {
    // A node without `runner:` falls back to `defaults.runner`.
    let Some(role) = ctx.manifest.config.runner_role(node) else {
        let end = super::check_exec::unset(ctx, node, yunta_core::ConfigKey::Runner).await?;
        return Ok(Step::Ended(end));
    };

    // A run pinned to an adapter takes only the candidates on it; a run on
    // the mock pins none and takes the first available candidate.
    match resolve_runner(
        role,
        &ctx.manifest.config,
        &|adapter| ctx.adapters.contains_key(adapter),
        ctx.manifest.adapter.as_ref().and_then(RunAdapter::pinned),
    ) {
        Ok(resolved) => {
            let mut chosen = resolved.chosen.clone();
            // The node's own `agent:` and `effort:` win over the
            // candidate's.
            if let Some(agent) = &node.agent {
                chosen.agent = Some(agent.clone());
            }
            chosen.effort = node.effort.or(chosen.effort);
            // An adapter without `custom_agents` fails the node
            // rather than silently dropping the requested agent.
            if chosen.agent.is_some() {
                let has_custom_agents = ctx.adapters.get(&chosen.adapter).is_some_and(|adapter| {
                    adapter
                        .capabilities()
                        .declares(yunta_core::Capability::CustomAgents)
                });
                if !has_custom_agents {
                    let failure = Failure::CapabilityMissing {
                        node: node.id.clone(),
                        capability: yunta_core::Capability::CustomAgents,
                        adapter: chosen.adapter.clone(),
                        used_for: None,
                        agent: chosen.agent.clone(),
                    };
                    let end = fail(ctx, node, failure).await?;
                    return Ok(Step::Ended(end));
                }
            }
            ctx.emit(
                Some(&node.id),
                EventPayload::Node(NodeEvent::RunnerResolved(RunnerResolvedPayload {
                    runner: resolved.runner.clone(),
                    chosen: chosen.clone(),
                    discarded: resolved.discarded.clone(),
                })),
            )
            .await?;
            Ok(Step::Value(chosen))
        }
        Err(error) => {
            let failure = runner_failure(error);
            Ok(Step::Ended(fail(ctx, node, failure).await?))
        }
    }
}

/// The failure a node leaves when its runner does not resolve: the runner it
/// asked for, and why no candidate of it can be used.
fn runner_failure(error: RunnerError) -> Failure {
    let (runner, runner_refusal) = match error {
        RunnerError::UnknownRunner { runner } => (runner, RunnerRefusal::Unknown),
        RunnerError::NoCandidateAvailable { runner, tried } => {
            (runner, RunnerRefusal::NoCandidate { tried })
        }
    };
    Failure::RunnerUnresolved {
        runner,
        runner_refusal,
    }
}

/// A blackboard group's session that cannot reach the per-run MCP
/// endpoint — the node fails with it, never emulates.
#[derive(Debug, thiserror::Error)]
pub enum RunToolsSetupError {
    #[error(
        "node `{node}` is in a `coordination: blackboard` group but adapter `{adapter}` declares \
         no `run_tools` capability — the blackboard cannot be mounted; pick a runner on an \
         adapter that can be a client of the per-run MCP endpoint"
    )]
    NoRunToolsCapability {
        node: yunta_core::NodeId,
        adapter: AdapterId,
    },
    #[error(
        "node `{node}` is in a `coordination: blackboard` group but its per-run MCP listener \
         failed to start: {source}"
    )]
    ListenerFailed {
        node: yunta_core::NodeId,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "node `{node}` declares a `{kind}` artifact, which a session hands over \
         through the run tools, and adapter `{adapter}` declares no `run_tools` capability — \
         the document has no way in; pick a runner on an adapter that can be a client of the \
         per-run MCP endpoint"
    )]
    TypedArtifactNeedsRunTools {
        node: yunta_core::NodeId,
        kind: yunta_core::ArtifactKind,
        adapter: AdapterId,
    },
    #[error(
        "node `{node}` declares a `{kind}` artifact, which a session hands over \
         through the run tools, and its per-run MCP listener failed to start: {source}"
    )]
    TypedArtifactListenerFailed {
        node: yunta_core::NodeId,
        kind: yunta_core::ArtifactKind,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "node `{node}` is a loop, whose task sessions read their task and check their work \
         through the run tools, and adapter `{adapter}` declares no `run_tools` capability — \
         a session there could not learn what its task asks; pick a runner on an adapter \
         that can be a client of the per-run MCP endpoint"
    )]
    TaskNeedsRunTools {
        node: yunta_core::NodeId,
        adapter: AdapterId,
    },
    #[error(
        "node `{node}` is a loop, whose task sessions read their task through the run tools, \
         and its per-run MCP listener failed to start: {source}"
    )]
    TaskListenerFailed {
        node: yunta_core::NodeId,
        #[source]
        source: std::io::Error,
    },
}

impl RunToolsSetupError {
    /// The failure this error leaves on its node. A listener that did not
    /// start is an engine step that failed on its own; a capability the
    /// adapter lacks is the adapter's, and says what the node needs it for.
    pub(crate) fn failure(&self) -> Failure {
        match self {
            RunToolsSetupError::NoRunToolsCapability { node, adapter } => {
                capability_missing(node, adapter, CapabilityUse::Blackboard)
            }
            RunToolsSetupError::TypedArtifactNeedsRunTools {
                node,
                adapter,
                kind,
            } => capability_missing(node, adapter, CapabilityUse::TypedArtifact { kind: *kind }),
            RunToolsSetupError::TaskNeedsRunTools { node, adapter } => {
                capability_missing(node, adapter, CapabilityUse::LoopTasks)
            }
            listener => Failure::EngineFailed {
                engine_error: listener.to_string(),
            },
        }
    }
}

/// The run tools' capability is missing from `adapter`, for what the node
/// needs them for.
fn capability_missing(node: &NodeId, adapter: &AdapterId, used_for: CapabilityUse) -> Failure {
    Failure::CapabilityMissing {
        node: node.clone(),
        capability: yunta_core::Capability::RunTools,
        adapter: adapter.clone(),
        used_for: Some(used_for),
        agent: None,
    }
}

/// Whether this node's sessions may mount the run tools, or why they
/// must not open at all.
///
/// The adapter's capability decides, and what the node declared decides
/// what its absence costs: a document that reaches the engine through
/// these tools and nowhere else, a `coordination: blackboard` group
/// whose semantics the engine never emulates, or a loop whose task
/// sessions read their task nowhere else, is a refusal before any token
/// is spent; anything else runs without them.
///
/// Asked once per node, and answered without binding anything: a
/// listener belongs to a session, and a node that opens many owns none
/// of them itself.
pub(crate) fn run_tools_allowed(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
) -> Result<(), RunToolsSetupError> {
    if adapter
        .capabilities()
        .declares(yunta_core::Capability::RunTools)
    {
        return Ok(());
    }
    let need = crate::task_cycle::RunToolsNeed::of(
        node,
        ctx.run_tools_host.is_blackboard_member(&node.id),
        &crate::run::node_exec::declared_artifacts(ctx, node),
    );
    let (node, adapter) = (node.id.clone(), adapter.id().clone());
    match need {
        None => Ok(()),
        Some(crate::task_cycle::RunToolsNeed::Blackboard) => {
            Err(RunToolsSetupError::NoRunToolsCapability { node, adapter })
        }
        Some(crate::task_cycle::RunToolsNeed::TypedArtifact(kind)) => {
            Err(RunToolsSetupError::TypedArtifactNeedsRunTools {
                node,
                kind,
                adapter,
            })
        }
        Some(crate::task_cycle::RunToolsNeed::Task) => {
            Err(RunToolsSetupError::TaskNeedsRunTools { node, adapter })
        }
    }
}

/// Records that a node's `network: false` is declarative only when the
/// session's resolved adapter cannot enforce it: before the
/// session, the engine states it will not sandbox the network — it never
/// emulates isolation it does not have. A node that declares no network
/// policy (`None`) or one whose adapter isolates the network records nothing.
pub(super) async fn report_declarative_network(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
) -> Result<(), RunError> {
    // Only an explicit `network: false` asks for isolation; a node that
    // never mentions the network declares no policy to degrade.
    if node.network == Some(false) {
        crate::run::capability::require(
            ctx,
            adapter,
            yunta_core::Capability::NetworkIsolation,
            node,
        )
        .await?;
    }
    Ok(())
}

//! Resolving a node's runner into an adapter, model and agent, opening
//! its per-run tools, and recording what the adapter cannot enforce.

use yunta_core::events::{EventPayload, RunnerResolvedPayload};
use yunta_core::{AdapterId, AgentName, Node};

use crate::runner::resolve_runner;

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
    let default_runner = ctx
        .manifest
        .config
        .defaults
        .as_ref()
        .and_then(|defaults| defaults.runner.as_ref());
    let Some(role) = node.runner.as_ref().or(default_runner) else {
        let end = fail(
            ctx,
            node,
            format!(
                "node `{}` has no `runner:` and the config declares no `defaults.runner` — \
                 declare one",
                node.id
            ),
            false,
        )
        .await?;
        return Ok(Step::Ended(end));
    };

    match resolve_runner(
        role,
        &ctx.manifest.config,
        &|adapter| ctx.adapters.contains_key(adapter),
        ctx.adapter_override,
    ) {
        Ok(resolved) => {
            let mut chosen = resolved.chosen.clone();
            // The node's own `agent:` wins over the
            // candidate's.
            if let Some(agent) = &node.agent {
                chosen.agent = Some(agent.clone());
            }
            // An adapter without `custom_agents` fails the node
            // rather than silently dropping the requested agent.
            if chosen.agent.is_some() {
                let has_custom_agents = ctx.adapters.get(&chosen.adapter).is_some_and(|adapter| {
                    adapter
                        .capabilities()
                        .declares(yunta_core::Capability::CustomAgents)
                });
                if !has_custom_agents {
                    let end = fail(
                        ctx,
                        node,
                        format!(
                            "node `{}` requests agent `{}` but adapter `{}` does not declare \
                             `custom_agents` — pick a candidate on an adapter that does, or \
                             drop the agent",
                            node.id,
                            chosen.agent.as_ref().map_or("", AgentName::as_str),
                            chosen.adapter
                        ),
                        false,
                    )
                    .await?;
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
        Err(e) => Ok(Step::Ended(fail(ctx, node, e.to_string(), false).await?)),
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
}

/// The first interpreted artifact this node declares, if any: the one a
/// refusal names, so a reader has somewhere to look.
pub(crate) fn declared_typed_artifact(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Option<yunta_core::ArtifactKind> {
    crate::run::node_exec::declared_artifacts(ctx, node)
        .into_iter()
        .find_map(|spec| spec.kind())
}

/// Whether this node's sessions may mount the run tools, or why they
/// must not open at all.
///
/// The adapter's capability decides, and what the node declared decides
/// what its absence costs: a document that reaches the engine through
/// these tools and nowhere else, or a `coordination: blackboard` group
/// whose semantics the engine never emulates, is a refusal before any
/// token is spent; anything else runs without them.
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
    // The blackboard's own reason comes first: it is the older one, and
    // a node can owe both.
    if ctx.run_tools_host.is_blackboard_member(&node.id) {
        return Err(RunToolsSetupError::NoRunToolsCapability {
            node: node.id.clone(),
            adapter: adapter.id().clone(),
        });
    }
    if let Some(kind) = declared_typed_artifact(ctx, node) {
        return Err(RunToolsSetupError::TypedArtifactNeedsRunTools {
            node: node.id.clone(),
            kind,
            adapter: adapter.id().clone(),
        });
    }
    Ok(())
}

/// Records that a node's `network: false` is declarative only when the
/// session's resolved adapter cannot enforce it (D105/D119): before the
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

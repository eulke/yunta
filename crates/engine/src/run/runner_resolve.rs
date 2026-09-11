//! Resolving a node's runner into an adapter, model and agent, opening
//! its per-run tools, and recording what the adapter cannot enforce.

use yunta_core::events::{EventPayload, RunnerResolvedPayload};
use yunta_core::{AdapterId, AgentName, Node};

use crate::runner::resolve_runner;

use super::node_close::fail;
use super::step::Step;
use super::{RunCtx, RunError};

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
                EventPayload::RunnerResolved(RunnerResolvedPayload {
                    runner: resolved.runner.clone(),
                    chosen: chosen.clone(),
                    discarded: resolved.discarded.clone(),
                }),
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
pub(super) enum RunToolsSetupError {
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
}

/// What [`open_run_tools`] resolved. `session` is the listener when one
/// opened; `degraded` carries the reason to record when the session
/// proceeds without run tools — the caller emits that
/// `capability_degraded` on the run's log, since this function has no
/// fallible emit of its own.
pub(super) struct RunToolsResolution {
    pub session: Option<crate::run_tools::RunToolsSession>,
    pub degraded: Option<String>,
}

/// Opens this session attempt's per-run MCP listener, or decides
/// it must not exist. A resolution with no session and no degradation —
/// no `run_tools` capability outside a blackboard group — is the resting
/// state. A resolution carrying `degraded` is the recorded fallback: the
/// listener could not bind but the node can proceed without it.
/// `Err(diagnostic)` is the fatal case: the node's group declared
/// `coordination: blackboard` and this session cannot carry it
/// (capability missing, or the listener failed to bind) — the caller
/// fails the node with it, never emulates.
pub(super) async fn open_run_tools(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_adapters::Adapter,
    adapter_id: &AdapterId,
    task: Option<&yunta_core::TaskId>,
) -> Result<RunToolsResolution, RunToolsSetupError> {
    let host = &ctx.run_tools_host;
    let needs_blackboard = host.is_blackboard_member(&node.id);
    if !adapter
        .capabilities()
        .declares(yunta_core::Capability::RunTools)
    {
        if needs_blackboard {
            return Err(RunToolsSetupError::NoRunToolsCapability {
                node: node.id.clone(),
                adapter: adapter_id.clone(),
            });
        }
        return Ok(RunToolsResolution {
            session: None,
            degraded: None,
        });
    }
    match crate::run_tools::open_session_listener(
        crate::run_tools::RunToolsAccess {
            host: host.clone(),
            node: node.id.clone(),
            declared: crate::run::node_exec::declared_artifacts(ctx, node),
        },
        task.cloned(),
        ctx.worktree.to_path_buf(),
    )
    .await
    {
        Ok(session) => Ok(RunToolsResolution {
            session: Some(session),
            degraded: None,
        }),
        Err(e) => {
            if needs_blackboard {
                return Err(RunToolsSetupError::ListenerFailed {
                    node: node.id.clone(),
                    source: e,
                });
            }
            Ok(RunToolsResolution {
                session: None,
                degraded: Some(format!("the session runs without run tools: {e}")),
            })
        }
    }
}

/// Records that a node's `network: false` is declarative only when the
/// session's resolved adapter cannot enforce it (D105/D119): before the
/// session, the engine states it will not sandbox the network — it never
/// emulates isolation it does not have. A node that declares no network
/// policy (`None`) or one whose adapter isolates the network records nothing.
pub(super) async fn report_declarative_network(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_adapters::Adapter,
    adapter_id: &AdapterId,
) -> Result<(), RunError> {
    if node.network == Some(false)
        && !adapter
            .capabilities()
            .declares(yunta_core::Capability::NetworkIsolation)
    {
        ctx.emit(
            Some(&node.id),
            EventPayload::CapabilityDegraded(yunta_core::events::CapabilityDegradedPayload {
                capability: yunta_core::Capability::NetworkIsolation,
                adapter: adapter_id.clone(),
                policy_applied: "declarative-only — the adapter declares no network isolation; \
                                 `network: false` is recorded for policy and audit, not enforced"
                    .to_string(),
            }),
        )
        .await?;
    }
    Ok(())
}

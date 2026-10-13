//! What a node's sessions may do beside writing their scope: the agent
//! they work as, and whose configuration of the agent's CLI they load.
//! Each is the node's own word, else the workflow's, else a default.

use yunta_core::agent::AgentDefinition;
use yunta_core::Node;

use super::node_exec::NodeEnd;

/// Whose CLI configuration `node`'s sessions load: its own word, else the
/// workflow's `node_defaults`, else the person's own.
pub(super) fn cli_config(ctx: &crate::run::RunCtx<'_>, node: &Node) -> yunta_core::CliConfig {
    node.cli_config
        .or_else(|| {
            ctx.manifest
                .workflow
                .node_defaults
                .as_ref()
                .and_then(|defaults| defaults.cli_config)
        })
        .unwrap_or_default()
}

/// The definition of the agent the node runs as, if it names one. A name
/// that resolves to no definition fails the node before any session
/// spends a token on it.
pub(super) async fn agent(
    ctx: &crate::run::RunCtx<'_>,
    node: &Node,
    chosen: &yunta_core::RunnerCandidate,
) -> Result<Result<Option<AgentDefinition>, NodeEnd>, crate::run::RunError> {
    let Some(name) = &chosen.agent else {
        return Ok(Ok(None));
    };
    match crate::agents::resolve_agent(name, ctx.worktree).await {
        Ok(agent) => Ok(Ok(Some(agent))),
        Err(error) => Ok(Err(crate::run::node_close::fail(
            ctx,
            node,
            error.to_string(),
        )
        .await?)),
    }
}

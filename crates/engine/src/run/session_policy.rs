//! What a node's sessions may do beside writing their scope: the agent
//! they work as, whose configuration of the agent's CLI they load, and
//! whether the commands they run reach the network. Each is the node's own
//! word, else the workflow's or the project's, else a default.

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

/// Whether `node`'s sessions' commands may reach the network: its own
/// `network:`, else the project's `permissions.network.default`, else
/// open.
pub(super) fn network(ctx: &crate::run::RunCtx<'_>, node: &Node) -> bool {
    node.network
        .or_else(|| {
            ctx.manifest
                .config
                .permissions
                .as_ref()
                .and_then(|permissions| permissions.network)
                .map(|network| network.default)
        })
        .unwrap_or(true)
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
            false,
        )
        .await?)),
    }
}

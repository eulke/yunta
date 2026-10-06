//! What a node's sessions may do beside writing their scope: whose
//! configuration of the agent's CLI they load, and whether the commands
//! they run reach the network. Each is the node's own word, else the
//! workflow's or the project's, else a default.

use yunta_core::Node;

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

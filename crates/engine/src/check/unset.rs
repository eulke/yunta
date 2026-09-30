//! See [`super`]. The config keys each node cannot run without.

use super::*;
use yunta_core::ConfigKey;

/// Every node whose kind cannot run with a key the config leaves unset:
/// a coverage gate with no `coverage`, an executor nobody registered, a
/// session with no runner to open on, a command the project does not
/// declare. A hook of `node_defaults:` runs on every node that declares
/// none of its own, so a command it names is refused under that block.
pub(crate) fn check_unset_keys(
    workflow: &Workflow,
    config: &ConfigLayer,
    errors: &mut Vec<CheckError>,
) {
    for node in workflow.iter_nodes() {
        for key in ConfigKey::unset(node, config) {
            errors.push(CheckError::Unset {
                node: node.id.clone(),
                key,
            });
        }
    }
    let default_hooks = workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref());
    let mut named: Vec<&yunta_core::CommandName> = Vec::new();
    for step in default_hooks
        .iter()
        .flat_map(|hooks| hooks.before.iter().chain(&hooks.after))
    {
        if let Some(command) = step.run.project() {
            if config.command(command).is_none() && !named.contains(&command) {
                named.push(command);
            }
        }
    }
    for command in named {
        errors.push(CheckError::Unset {
            node: NODE_DEFAULTS.clone(),
            key: ConfigKey::Command {
                command: command.clone(),
            },
        });
    }
}

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
    // A node the run would leave out needs nothing: the project not
    // providing it is what leaves it out.
    let left_out: Vec<NodeId> = yunta_core::left_out(workflow, config)
        .into_iter()
        .map(|left| left.node)
        .collect();
    for (node, group) in workflow.iter_nodes_with_group() {
        if left_out.contains(group.map_or(&node.id, |group| &group.id)) {
            continue;
        }
        for key in ConfigKey::unset(node, config) {
            errors.push(CheckError::Unset {
                node: node.id.clone(),
                near: near_key(&key, config),
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
        let key = ConfigKey::Command {
            command: command.clone(),
        };
        errors.push(CheckError::Unset {
            node: NODE_DEFAULTS.clone(),
            near: near_key(&key, config),
            key,
        });
    }
}

/// The command `config` declares that the one `key` asks for most likely
/// misspells. Only a command has a name a person types: the other keys
/// are sections, present or not.
fn near_key(key: &ConfigKey, config: &ConfigLayer) -> Option<String> {
    let ConfigKey::Command { command } = key else {
        return None;
    };
    let declared = config.commands.as_ref()?;
    yunta_core::text::nearest(command.as_str(), declared.keys().map(|name| name.as_str()))
        .map(str::to_string)
}

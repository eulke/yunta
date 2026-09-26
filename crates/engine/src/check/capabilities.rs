//! See [`super`]. What a workflow asks of its adapters, checked before a
//! run is born.
//!
//! Two of a node's declarations are the adapter's to honor or not:
//! `permissions:` needs `permission_profiles`, and `agent:` needs
//! `custom_agents`. Neither has a fallback — a session that silently
//! ignored the profile it was given would edit under permissions nobody
//! granted — so the workflow is refused here rather than at the node
//! that would have run under the wrong ones.

use super::*;
use yunta_core::port::{absence_of, Absence};
use yunta_core::{AdapterId, Capabilities, Capability, RunnerName};

/// Every node's declarations checked against the adapters its runner
/// could resolve to. `declared` answers for an adapter this binary
/// built; an adapter it knows nothing about is judged by nothing, since
/// a capability it cannot see is not a capability it can call absent.
///
/// A node whose runner fans out over several adapters is refused only
/// when *none* of them can do what it declares: the run picks the first
/// available candidate, so one that cannot is short of a workflow that
/// cannot run.
pub(crate) fn check_adapter_capabilities(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&AdapterId) -> Option<Capabilities>,
    errors: &mut Vec<CheckError>,
) {
    for node in workflow.iter_nodes() {
        let adapters = candidate_adapters(node, config);
        if node.permissions.is_some() {
            refuse_if_none_can(
                node,
                &adapters,
                declared,
                Capability::PermissionProfiles,
                "permissions:",
                errors,
            );
        }
        if node.agent.is_some() {
            refuse_if_none_can(
                node,
                &adapters,
                declared,
                Capability::CustomAgents,
                "agent:",
                errors,
            );
        }
    }
}

/// Records a refusal when the node declares `field`, at least one of its
/// candidate adapters is known, and not one of the known ones declares
/// `capability`.
fn refuse_if_none_can(
    node: &Node,
    adapters: &[AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<Capabilities>,
    capability: Capability,
    field: &str,
    errors: &mut Vec<CheckError>,
) {
    debug_assert_eq!(
        absence_of(capability),
        &Absence::FailAtCheck,
        "only a capability whose absence fails at check is refused here"
    );
    let known: Vec<(&AdapterId, Capabilities)> = adapters
        .iter()
        .filter_map(|adapter| declared(adapter).map(|capabilities| (adapter, capabilities)))
        .collect();
    if known.is_empty() || known.iter().any(|(_, caps)| caps.declares(capability)) {
        return;
    }
    errors.push(CheckError::CapabilityUnsupported {
        node: node.id.clone(),
        field: field.to_string(),
        capability,
        adapters: known
            .iter()
            .map(|(adapter, _)| adapter.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    });
}

/// Every adapter this node's runner could resolve to, in declaration
/// order. Empty when the node names no runner the config defines.
fn candidate_adapters(node: &Node, config: &ConfigLayer) -> Vec<AdapterId> {
    let names: Vec<&RunnerName> = if !node.runners.is_empty() {
        node.runners.iter().collect()
    } else {
        node.runner
            .as_ref()
            .or(config.defaults.as_ref().and_then(|d| d.runner.as_ref()))
            .into_iter()
            .collect()
    };
    let Some(runners) = config.runners.as_ref() else {
        return Vec::new();
    };
    names
        .into_iter()
        .filter_map(|name| runners.get(name))
        .flatten()
        .map(|candidate| candidate.adapter.clone())
        .collect()
}

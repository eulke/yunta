//! See [`super`]. What a workflow asks of its adapters, checked before a
//! run is born.
//!
//! Three of a node's declarations are the adapter's to honor or not:
//! `permissions:` needs `permission_profiles`, `agent:` needs
//! `custom_agents`, and what a session can only hand over or read through
//! the engine — a document, a task, a blackboard — needs `run_tools`. A
//! prompt or loop also needs `stop_refusals`: the engine may refuse a
//! session's stop, and the CLI must keep the session going past it. None
//! has a fallback — a session that silently ignored the profile it was
//! given would edit under permissions nobody granted, one without the run
//! tools has no way to hand over what it declared, and one that ends at a
//! refused stop leaves its work short — so the workflow is refused here
//! rather than at the node that would have run without it.
//!
//! An `edit` session's `sandbox` is the adapter's setting, not a capability:
//! under a read-only one the session is given no network, so a node that
//! opens a session and asks for the network is refused where the sandbox is
//! known. The run's token cap is the other adapter-aware rule: it is checked
//! when a session ends for an adapter that declares no `mid_session_token_cap`,
//! and [`check_token_cap_warnings`] names those nodes.

use super::*;
use yunta_core::port::{absence_of, Absence, PermissionProfile};
use yunta_core::{
    AdapterId, Capabilities, Capability, EditSandbox, NodePermissions, RunnerName, Worktree,
};

/// What `check` knows of an adapter this binary builds: what it declares,
/// and the `sandbox` its settings give an `edit` session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterFacts {
    pub capabilities: Capabilities,
    pub edit_sandbox: EditSandbox,
}

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
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
    errors: &mut Vec<CheckError>,
) {
    for (node, group) in workflow.iter_nodes_with_group() {
        let adapters = candidate_adapters(node, config);
        check_profile_and_agent(node, &adapters, declared, errors);
        check_network_under_read_only(node, &adapters, declared, errors);
        let blackboard_member = group.is_some_and(|group| {
            matches!(
                group.kind,
                NodeKind::Parallel {
                    coordination: yunta_core::Coordination::Blackboard,
                    ..
                }
            )
        });
        let declared_artifacts = node
            .artifacts
            .as_ref()
            .map(|artifacts| artifacts.produces.as_slice())
            .unwrap_or_default();
        if let Some(need) =
            crate::task_cycle::RunToolsNeed::of(node, blackboard_member, declared_artifacts)
        {
            refuse_if_none_can(
                node,
                &adapters,
                declared,
                Capability::RunTools,
                &need.declaration(),
                errors,
            );
        }
        check_stop_refusals(node, &adapters, declared, errors);
    }
}

/// A node's permission profile and custom agent each need a candidate
/// adapter that can apply them.
fn check_profile_and_agent(
    node: &Node,
    adapters: &[AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
    errors: &mut Vec<CheckError>,
) {
    if node.permissions.is_some() {
        refuse_if_none_can(
            node,
            adapters,
            declared,
            Capability::PermissionProfiles,
            "permissions:",
            errors,
        );
    }
    if node.agent.is_some() {
        refuse_if_none_can(
            node,
            adapters,
            declared,
            Capability::CustomAgents,
            "agent:",
            errors,
        );
    }
}

/// A node that asks for the network and runs read-only is given none, so
/// its request cannot hold. A `read-only` node is refused on its own word;
/// an `edit` session is refused when every known candidate runs it under a
/// read-only `sandbox`. A node that opens no session has no session to run
/// read-only, so it is not judged here.
fn check_network_under_read_only(
    node: &Node,
    adapters: &[AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
    errors: &mut Vec<CheckError>,
) {
    if node.network != Some(true) || !node.opens_session() {
        return;
    }
    let profile = node
        .permissions
        .map_or(PermissionProfile::Edit, NodePermissions::profile);
    let read_only = match profile {
        PermissionProfile::ReadOnly => true,
        PermissionProfile::Edit => all_known_hold(adapters, declared, |facts| {
            Worktree::of(profile, facts.edit_sandbox, false) == Worktree::Nothing
        }),
        PermissionProfile::Full => false,
    };
    if read_only {
        errors.push(CheckError::NetworkOpenUnderReadOnly {
            node: node.id.clone(),
        });
    }
}

/// A prompt or loop node runs a session whose stop the engine can refuse,
/// so one of its candidate adapters must keep such a session going.
fn check_stop_refusals(
    node: &Node,
    adapters: &[AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
    errors: &mut Vec<CheckError>,
) {
    if node.opens_session() {
        refuse_if_none_can(
            node,
            adapters,
            declared,
            Capability::StopRefusals,
            &format!("kind: {}", node.kind.kind_name()),
            errors,
        );
    }
}

/// Records a refusal when the node declares `field`, at least one of its
/// candidate adapters is known, and not one of the known ones declares
/// `capability`.
fn refuse_if_none_can(
    node: &Node,
    adapters: &[AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
    capability: Capability,
    field: &str,
    errors: &mut Vec<CheckError>,
) {
    debug_assert_eq!(
        absence_of(capability),
        &Absence::FailAtCheck,
        "only a capability whose absence fails at check is refused here"
    );
    let known = known_candidates(adapters, declared);
    if known.is_empty()
        || known
            .iter()
            .any(|(_, facts)| facts.capabilities.declares(capability))
    {
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
pub(crate) fn candidate_adapters(node: &Node, config: &ConfigLayer) -> Vec<AdapterId> {
    let names: Vec<&RunnerName> = if !node.runners.is_empty() {
        node.runners.iter().collect()
    } else {
        config.runner_role(node).into_iter().collect()
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

/// The candidates `declared` knows, each with its facts, in declaration
/// order.
fn known_candidates<'a>(
    adapters: &'a [AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
) -> Vec<(&'a AdapterId, AdapterFacts)> {
    adapters
        .iter()
        .filter_map(|adapter| declared(adapter).map(|facts| (adapter, facts)))
        .collect()
}

/// Whether at least one of `adapters` is known and every known one holds
/// `holds`. An adapter nothing knows is judged by nothing, so a node whose
/// candidates are all unknown is not refused on what they might be.
pub(crate) fn all_known_hold(
    adapters: &[AdapterId],
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
    holds: impl Fn(&AdapterFacts) -> bool,
) -> bool {
    let known = known_candidates(adapters, declared);
    !known.is_empty() && known.iter().all(|(_, facts)| holds(facts))
}

/// The run's `limits.max_tokens_per_run` is checked when a session ends, for
/// each prompt or loop whose known candidates declare no
/// `mid_session_token_cap`: such an adapter reports no usage mid-session, so
/// the cap cannot stop a session as it runs. Without a cap there is nothing
/// to check.
pub fn check_token_cap_warnings(
    workflow: &Workflow,
    config: &ConfigLayer,
    declared: &dyn Fn(&AdapterId) -> Option<AdapterFacts>,
) -> Vec<CheckWarning> {
    if config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_tokens_per_run)
        .is_none()
    {
        return Vec::new();
    }
    workflow
        .iter_nodes()
        .filter(|node| node.opens_session())
        .filter(|node| {
            all_known_hold(&candidate_adapters(node, config), declared, |facts| {
                !facts.capabilities.declares(Capability::MidSessionTokenCap)
            })
        })
        .map(|node| CheckWarning::TokenCapAtSessionEnd {
            node: node.id.clone(),
        })
        .collect()
}

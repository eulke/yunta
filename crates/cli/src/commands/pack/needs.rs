//! What an installed pack's workflows need from this project that its
//! config does not declare, said once the pack is vendored — with what
//! this repository was detected to answer for each — so a person learns
//! it before a run is refused for it, or leaves out a node for it.

use std::path::Path;

use yunta_core::{ConfigKey, NodeId, PackManifest, Workflow};

use crate::context::Context;
use crate::detect::Detected;
use crate::error::CliError;

/// What a node of an installed pack needs from this project and its config
/// does not declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Need {
    pub(super) node: NodeId,
    pub(super) key: ConfigKey,
    /// Whether a run leaves the node out until the project declares it; a
    /// run is refused for it otherwise.
    pub(super) leaves_node_out: bool,
    /// What this repository was detected to answer for it, when it was.
    pub(super) detected: Option<String>,
}

/// What `manifest`'s workflows, vendored at `pack_dir`, need and the
/// project lacks, each once; none when they need nothing.
pub(super) async fn unmet(
    ctx: &Context,
    pack_dir: &Path,
    manifest: &PackManifest,
) -> Result<Vec<Need>, CliError> {
    let detected = Detected::in_repo(&ctx.cwd, ctx.supervision()).await?;
    let mut unmet: Vec<Need> = Vec::new();
    for declared in &manifest.contents.workflows {
        let Ok(workflow) = crate::load_workflow(&pack_dir.join(declared)) else {
            continue;
        };
        for need in needs(&workflow, &ctx.project.config, &detected) {
            if !unmet.contains(&need) {
                unmet.push(need);
            }
        }
    }
    Ok(unmet)
}

/// Each key a node of `workflow` needs and `config` leaves unset, with
/// whether a run leaves the node out for it and what this repository
/// answers for it.
fn needs(workflow: &Workflow, config: &yunta_core::ConfigLayer, detected: &Detected) -> Vec<Need> {
    let left_out: Vec<NodeId> = yunta_core::left_out(workflow, config)
        .into_iter()
        .map(|left| left.node)
        .collect();
    let mut needs = Vec::new();
    let mut need = |node: &NodeId, key: ConfigKey, leaves_node_out: bool| {
        needs.push(Need {
            node: node.clone(),
            detected: detected.suggestion(&key),
            key,
            leaves_node_out,
        });
    };
    for (node, group) in workflow.iter_nodes_with_group() {
        let optional = left_out.contains(group.map_or(&node.id, |group| &group.id));
        for key in ConfigKey::unset(node, config) {
            need(&node.id, key, optional);
        }
        let compares = matches!(
            node.kind,
            yunta_core::NodeKind::Check(yunta_core::CheckBuiltin::BaselineCompare)
        );
        if compares && !ConfigKey::BaselineSuite.is_declared(config) {
            need(&node.id, ConfigKey::BaselineSuite, optional);
        }
    }
    needs
}

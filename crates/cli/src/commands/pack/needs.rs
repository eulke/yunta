//! What an installed pack's workflows need from this project that its
//! config does not declare, read once the pack is vendored beside what this
//! repository was detected to have, which may answer it — so a person learns
//! it before a run is refused for it, or leaves out a node for it.

use std::path::Path;

use yunta_core::{ConfigKey, NodeId, PackManifest, Workflow};
use yunta_render::page::pack::{Need, Needs};

use crate::context::Context;
use crate::error::CliError;

/// What `manifest`'s workflows, vendored at `pack_dir`, need and the
/// project lacks, each once — none when they need nothing — and what this
/// repository was detected to have.
pub(super) async fn unmet(
    ctx: &Context,
    pack_dir: &Path,
    manifest: &PackManifest,
) -> Result<Needs, CliError> {
    let detected =
        yunta_api::machine::in_repo(&ctx.project.workspace, &ctx.cwd, &ctx.stop()).await?;
    let mut unmet: Vec<Need> = Vec::new();
    for declared in &manifest.contents.workflows {
        let Ok(workflow) = crate::load_workflow(&pack_dir.join(declared)) else {
            continue;
        };
        for need in workflow_needs(&workflow, &ctx.project.config) {
            if !unmet.contains(&need) {
                unmet.push(need);
            }
        }
    }
    Ok(Needs { unmet, detected })
}

/// Each key a node of `workflow` needs and `config` leaves unset, with
/// whether a run leaves the node out for it.
fn workflow_needs(workflow: &Workflow, config: &yunta_core::ConfigLayer) -> Vec<Need> {
    let left_out: Vec<NodeId> = yunta_core::left_out(workflow, config)
        .into_iter()
        .map(|left| left.node)
        .collect();
    let mut needs = Vec::new();
    let mut need = |node: &NodeId, key: ConfigKey, leaves_node_out: bool| {
        needs.push(Need {
            node: node.clone(),
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

//! What an installed pack's workflows need from this project that its
//! config does not declare, said once the pack is vendored — with what
//! this repository was detected to answer for each — so a person learns
//! it before a run is refused for it, or leaves out a node for it.

use std::path::Path;

use yunta_core::{ConfigKey, NodeId, PackManifest, Workflow};

use crate::context::Context;
use crate::detect::Detected;

/// Prints what `manifest`'s workflows, vendored at `pack_dir`, need and
/// the project lacks; nothing when they need nothing.
pub(crate) async fn report(ctx: &Context, pack_dir: &Path, manifest: &PackManifest) {
    let detected = Detected::in_repo(&ctx.cwd, ctx.supervision()).await;
    let mut lines: Vec<String> = Vec::new();
    for declared in &manifest.contents.workflows {
        let Ok(workflow) = crate::load_workflow(&pack_dir.join(declared)) else {
            continue;
        };
        for line in needs(&workflow, &ctx.project.config, &detected) {
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
    }
    if lines.is_empty() {
        return;
    }
    println!("\n{} needs from this project:", manifest.reference());
    for line in lines {
        println!("  {line}");
    }
}

/// One line per key `workflow` needs and `config` leaves unset — saying
/// whether a run is refused for it or leaves the node out — each
/// followed by what this repository answers for it, when it does.
fn needs(
    workflow: &Workflow,
    config: &yunta_core::ConfigLayer,
    detected: &Detected,
) -> Vec<String> {
    let left_out: Vec<NodeId> = yunta_core::left_out(workflow, config)
        .into_iter()
        .map(|left| left.node)
        .collect();
    let mut lines = Vec::new();
    let mut need = |node: &NodeId, key: ConfigKey, optional: bool| {
        let fate = match optional {
            true => "a run leaves the node out until it does",
            false => "a run is refused until it does",
        };
        lines.push(format!(
            "node `{node}`: the project declares {} — {fate}",
            key.undeclared()
        ));
        if let Some(suggestion) = detected.suggestion(&key) {
            lines.push(format!("  {suggestion}"));
        }
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
    lines
}

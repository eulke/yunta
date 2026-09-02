//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Walks the composition reference graph as the repo's
/// catalog stands **today** — every `use:` resolves, no cycles, and
/// nesting stays within `limits.max_workflow_depth`. A separate entry
/// point from [`check`], deliberately: `check` never reads files (its
/// own doc-comment rule), while this walk exists precisely to read the
/// catalog — the CLI calls both.
pub fn check_workflow_refs(
    workflow: &Workflow,
    config: &ConfigLayer,
    repo_root: &std::path::Path,
    workflow_origin: &crate::catalog::WorkflowOrigin,
) -> Vec<CheckError> {
    let mut errors = check_declares_ceiling(workflow, workflow_origin, repo_root);
    let max_depth = config.resolved_max_workflow_depth();
    let mut path: Vec<String> = Vec::new();
    walk_workflow_refs(
        workflow,
        repo_root,
        workflow_origin,
        max_depth,
        &mut path,
        &mut errors,
    );
    errors
}

/// Every `(node, use-name)` reference, `parallel` children included.
pub(crate) fn workflow_uses(workflow: &Workflow) -> Vec<(NodeId, String)> {
    workflow
        .iter_nodes()
        .filter_map(|node| match &node.kind {
            NodeKind::Workflow { r#use, .. } => Some((node.id.clone(), r#use.clone())),
            _ => None,
        })
        .collect()
}

pub(crate) fn walk_workflow_refs(
    workflow: &Workflow,
    repo_root: &std::path::Path,
    current_origin: &crate::catalog::WorkflowOrigin,
    max_depth: u32,
    path: &mut Vec<String>,
    errors: &mut Vec<CheckError>,
) {
    use crate::catalog::{resolve_workflow, CatalogError, WorkflowOrigin};

    for (node, name) in workflow_uses(workflow) {
        if path.contains(&name) {
            let chain = path
                .iter()
                .cloned()
                .chain(std::iter::once(name.clone()))
                .collect::<Vec<_>>()
                .join(" -> ");
            errors.push(CheckError::WorkflowRefCycle { chain });
            continue;
        }
        let depth = path.len() as u32 + 1;
        if depth > max_depth {
            let chain = path
                .iter()
                .cloned()
                .chain(std::iter::once(name.clone()))
                .collect::<Vec<_>>()
                .join(" -> ");
            errors.push(CheckError::WorkflowRefTooDeep {
                chain,
                depth,
                max: max_depth,
            });
            continue;
        }
        let resolved = match resolve_workflow(repo_root, &name) {
            Ok(resolved) => resolved,
            Err(e @ CatalogError::Ambiguous { .. }) => {
                errors.push(CheckError::AmbiguousWorkflowRef {
                    node,
                    name,
                    detail: e.to_string(),
                });
                continue;
            }
            Err(e) => {
                errors.push(CheckError::WorkflowRefMissing {
                    node,
                    name,
                    detail: e.to_string(),
                });
                continue;
            }
        };

        // Composing from inside a pack may only reach other
        // workflows in that *same* pack — never back out to the repo,
        // never sideways into a different pack (no transitive pack
        // dependencies).
        if let WorkflowOrigin::Pack {
            publisher,
            pack_name,
        } = current_origin
        {
            let same_pack = matches!(
                &resolved.origin,
                WorkflowOrigin::Pack { publisher: p2, pack_name: n2 }
                    if p2 == publisher && n2 == pack_name
            );
            if !same_pack {
                errors.push(CheckError::CrossPackWorkflowRef {
                    node,
                    name,
                    from_pack: format!("{publisher}/{pack_name}"),
                });
                continue;
            }
        }

        let text = match std::fs::read_to_string(&resolved.path) {
            Ok(text) => text,
            Err(_) => {
                errors.push(CheckError::WorkflowRefMissing {
                    node,
                    name,
                    detail: format!("`{}` doesn't exist", resolved.path.display()),
                });
                continue;
            }
        };
        let child: Workflow = match yunta_core::yaml::parse(&text) {
            Ok(child) => child,
            Err(e) => {
                errors.push(CheckError::WorkflowRefUnparseable {
                    path: resolved.path,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        errors.extend(check_declares_ceiling(&child, &resolved.origin, repo_root));
        path.push(name);
        walk_workflow_refs(&child, repo_root, &resolved.origin, max_depth, path, errors);
        path.pop();
    }
}

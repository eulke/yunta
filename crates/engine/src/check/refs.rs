//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Walks the composition reference graph as the repo's
/// catalog stands **today** — every `use:` resolves, no cycles, and
/// nesting stays within `limits.max_workflow_depth`. A separate entry
/// point from [`check`], deliberately: `check` never reads files (its
/// own doc-comment rule), while this walk exists precisely to read the
/// catalog — the CLI calls both.
///
/// Every workflow the composition reaches is also checked as the run
/// that composes it would check it at birth — against the same config,
/// holding what its node mounts — so a child that would be refused
/// halfway through the parent's run is refused before the parent spends
/// anything.
pub fn check_workflow_refs(
    workflow: &Workflow,
    config: &ConfigLayer,
    repo_root: &std::path::Path,
    workflow_origin: &crate::catalog::WorkflowOrigin,
    declared: &dyn Fn(&yunta_core::AdapterId) -> Option<yunta_core::Capabilities>,
) -> RefsCheck {
    let mut errors = check_declares_ceiling(workflow, workflow_origin, repo_root);
    let max_depth = config.resolved_max_workflow_depth();
    let mut path: Vec<String> = Vec::new();
    let mut compares = comparisons(workflow, &path);
    walk_workflow_refs(
        workflow,
        Walk {
            repo_root,
            config,
            declared,
            max_depth,
        },
        workflow_origin,
        &mut path,
        &mut errors,
        &mut compares,
    );
    let mut warnings = Vec::new();
    // Only the walk can answer either: `check` reads no files, so it
    // cannot know whether a workflow this one composes compares. The
    // lineage measures once, before its first node, only when the config
    // names a suite and something reads the measurement — so a comparison
    // under a config that names none has nothing to compare against,
    // wherever it sits, and a suite is wasted only on a workflow that
    // composes others none of which compares, with no loop of its own.
    let measured_for_nothing = crate::run::baseline::reads_the_baseline(workflow)
        && !workflow
            .iter_nodes()
            .any(|node| matches!(node.kind, yunta_core::NodeKind::Loop { .. }));
    match &config.baseline {
        Some(baseline) if compares.is_empty() && measured_for_nothing => {
            warnings.push(CheckWarning::BaselineNeverCompared {
                suite: baseline.suite.clone(),
            });
        }
        None if !compares.is_empty() => {
            errors.push(CheckError::BaselineWithoutSuite {
                sites: compares.join(", "),
            });
        }
        _ => {}
    }
    RefsCheck { errors, warnings }
}

/// What the walk carries down unchanged to every level.
#[derive(Clone, Copy)]
pub(crate) struct Walk<'a> {
    repo_root: &'a std::path::Path,
    config: &'a ConfigLayer,
    declared: &'a dyn Fn(&yunta_core::AdapterId) -> Option<yunta_core::Capabilities>,
    max_depth: u32,
}

/// What the composition walk found: the errors that refuse the run, and
/// the warnings only a reader of the composed files can raise.
pub struct RefsCheck {
    pub errors: Vec<CheckError>,
    pub warnings: Vec<CheckWarning>,
}

/// Every node of `workflow` that compares against the baseline, named
/// with the composition `path` that reaches it.
fn comparisons(workflow: &Workflow, path: &[String]) -> Vec<String> {
    workflow
        .iter_nodes()
        .filter(|node| {
            matches!(
                &node.kind,
                NodeKind::Check(yunta_core::CheckBuiltin::BaselineCompare)
            )
        })
        .map(|node| match path {
            [] => format!("node `{}`", node.id),
            _ => format!("node `{}` of `{}`", node.id, path.join(" -> ")),
        })
        .collect()
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
    walk: Walk<'_>,
    current_origin: &crate::catalog::WorkflowOrigin,
    path: &mut Vec<String>,
    errors: &mut Vec<CheckError>,
    compares: &mut Vec<String>,
) {
    use crate::catalog::{resolve_workflow, CatalogError, WorkflowOrigin};
    let Walk {
        repo_root,
        config,
        declared,
        max_depth,
    } = walk;

    for (node, name) in workflow_uses(workflow) {
        if path.contains(&name) {
            let chain = path
                .iter()
                .cloned()
                .chain(std::iter::once(name.clone()))
                .collect::<Vec<_>>()
                .join(" -> ");
            errors.push(CheckError::Composition(Composition::Cycle { chain }));
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
            errors.push(CheckError::Composition(Composition::TooDeep {
                chain,
                depth,
                max: max_depth,
            }));
            continue;
        }
        let resolved = match resolve_workflow(repo_root, &name) {
            Ok(resolved) => resolved,
            Err(e @ CatalogError::Ambiguous { .. }) => {
                errors.push(CheckError::Composition(Composition::Ambiguous {
                    node,
                    name,
                    detail: e.to_string(),
                }));
                continue;
            }
            Err(e) => {
                errors.push(CheckError::Composition(Composition::Missing {
                    node,
                    name,
                    detail: e.to_string(),
                }));
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
                errors.push(CheckError::Composition(Composition::CrossPack {
                    node,
                    name,
                    from_pack: format!("{publisher}/{pack_name}"),
                }));
                continue;
            }
        }

        let text = match std::fs::read_to_string(&resolved.path) {
            Ok(text) => text,
            Err(_) => {
                errors.push(CheckError::Composition(Composition::Missing {
                    node,
                    name,
                    detail: format!("`{}` doesn't exist", resolved.path.display()),
                }));
                continue;
            }
        };
        let child = match yunta_core::workflow::read::read(&text, &resolved.path) {
            Ok(child) => child,
            Err(report) => {
                errors.push(CheckError::Composition(Composition::Unparseable {
                    path: resolved.path,
                    detail: report.to_string(),
                }));
                continue;
            }
        };
        errors.extend(check_declares_ceiling(&child, &resolved.origin, repo_root));
        let problems =
            crate::check::check_mounted(&child, config, declared, mounts_of(workflow, &node));
        if !problems.is_empty() {
            errors.push(CheckError::Composition(Composition::Fails {
                node: node.clone(),
                name: name.clone(),
                problems: problems
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            }));
        }
        path.push(name);
        compares.extend(comparisons(&child, path));
        walk_workflow_refs(&child, walk, &resolved.origin, path, errors, compares);
        path.pop();
    }
}

/// What the `kind: workflow` node `id` of `workflow` mounts into its child.
fn mounts_of<'a>(workflow: &'a Workflow, id: &NodeId) -> &'a [yunta_core::MountSpec] {
    workflow
        .iter_nodes()
        .find_map(|node| match &node.kind {
            NodeKind::Workflow { mounts, .. } if node.id == *id => Some(mounts.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

/// What refuses a `use:` reference: one that does not resolve, or
/// resolves to a workflow its birth would refuse, or composes into a
/// cycle or past the depth a run allows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Composition {
    /// A composition reference that can't resolve today — the
    /// same broken-reference class as `BrokenReference`, across
    /// files. Advisory about the *current* catalog by design: the child
    /// freezes its own file at birth, so a run only ever meets the file
    /// as it is then.
    #[error("node `{node}`: `use: {name}` cannot be resolved — {detail}")]
    Missing {
        node: NodeId,
        name: String,
        detail: String,
    },

    /// Two packs installed under the same publisher each declare a
    /// workflow with the same file basename — the flat
    /// `publisher/workflow` namespace can't tell them apart.
    #[error("node `{node}`: `use: {name}` is ambiguous — {detail}")]
    Ambiguous {
        node: NodeId,
        name: String,
        detail: String,
    },

    /// Cross-pack references aren't supported — a workflow
    /// that lives inside a pack may only `use:` other workflows from
    /// that same pack, never the repo's own catalog or a different
    /// pack (no transitive pack dependencies).
    #[error(
        "node `{node}`: `use: {name}` reaches outside pack `{from_pack}` — composition across \
         packs isn't supported; copy what you need into your own pack instead"
    )]
    CrossPack {
        node: NodeId,
        name: String,
        from_pack: String,
    },

    #[error("workflow `{path}` (referenced through composition) does not parse: {detail}")]
    Unparseable {
        path: std::path::PathBuf,
        detail: String,
    },

    /// A workflow this one composes that its birth would refuse, found
    /// before the run that would compose it spends anything.
    #[error("node `{node}`: `use: {name}` fails check — {problems}")]
    Fails {
        node: NodeId,
        name: String,
        problems: String,
    },

    /// The graph of references between workflows must be acyclic.
    #[error("workflow composition cycle: {chain}")]
    Cycle { chain: String },

    /// The configurable maximum nesting depth, checked statically over
    /// the reference graph (the runtime guard at child birth enforces
    /// the same limit over what actually loads).
    #[error(
        "workflow composition {chain} nests {} deep but `limits.max_workflow_depth` is \
         {max} — flatten the composition or raise the limit",
        yunta_core::text::counted(*depth as usize, "level")
    )]
    TooDeep { chain: String, depth: u32, max: u32 },
}

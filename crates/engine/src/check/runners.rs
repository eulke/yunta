//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Runner fan-out declaration rules, checked before expansion.
pub(crate) fn check_runner_fanout(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let fanout_ids: HashSet<&NodeId> = workflow
        .nodes
        .iter()
        .filter(|node| !node.runners.is_empty())
        .map(|node| &node.id)
        .collect();
    for node in &workflow.nodes {
        if !node.runners.is_empty() && node.runner.is_some() {
            errors.push(CheckError::BothRunnerAndRunners {
                node: node.id.clone(),
            });
        }
        if let Some(on_failure) = &node.on_failure {
            if fanout_ids.contains(&on_failure.goto) {
                errors.push(CheckError::FanOutTarget {
                    node: node.id.clone(),
                    target: on_failure.goto.clone(),
                });
            }
        }
        if let NodeKind::Gate { on, .. } = &node.kind {
            for target in on.values() {
                if fanout_ids.contains(target) {
                    errors.push(CheckError::FanOutTarget {
                        node: node.id.clone(),
                        target: target.clone(),
                    });
                }
            }
        }
        for spec in &node.context {
            if let yunta_core::ContextSpec::Artifact { artifact } = spec {
                if let Some(referenced) = &artifact.node {
                    if fanout_ids.contains(referenced) {
                        errors.push(CheckError::FanOutTarget {
                            node: node.id.clone(),
                            target: referenced.clone(),
                        });
                    }
                }
            }
        }
        if let NodeKind::Workflow { mounts, .. } = &node.kind {
            for mount in mounts {
                if fanout_ids.contains(&mount.artifact.node) {
                    errors.push(CheckError::MountOnFanOut {
                        node: node.id.clone(),
                        target: mount.artifact.node.clone(),
                    });
                }
            }
        }
    }
    // An explicit `runners: []` parses identically to an absent field
    // (Vec + serde default), so it degrades to the ordinary
    // no-runner-declared path — reported there, never silently special-
    // cased here.
}

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
        for read in yunta_core::workflow::reads::artifact_reads(node) {
            let Some(referenced) = read.node.filter(|named| fanout_ids.contains(*named)) else {
                continue;
            };
            let (node, target) = (node.id.clone(), referenced.clone());
            errors.push(match read.site {
                yunta_core::workflow::reads::ReadSite::Mount => {
                    CheckError::MountOnFanOut { node, target }
                }
                _ => CheckError::FanOutTarget { node, target },
            });
        }
    }
    // An explicit `runners: []` parses identically to an absent field
    // (Vec + serde default), so it degrades to the ordinary
    // no-runner-declared path — reported there, never silently special-
    // cased here.
}

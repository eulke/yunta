//! See [`super`]. One family of workflow-check rules.

use super::*;

/// `modes:`'s own three invariants — independent of the
/// mode's name or count, checked once per declared mode. `include: all`
/// is trivially coherent (everything's in it), so only the explicit
/// node-list form has anything to check.
pub(crate) fn check_modes(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    let Some(modes) = &workflow.modes else {
        return;
    };

    // Mode `include:` only ever names *top-level* nodes, never reaching
    // into a `parallel` group's children — a `parallel` group is
    // included or excluded as a whole, so "known" here deliberately
    // excludes nested child ids even though `check`'s other rules track
    // them for global uniqueness.
    let top_level_ids: HashSet<&NodeId> = workflow.nodes.iter().map(|n| &n.id).collect();
    let invariant_ids: Vec<&NodeId> = workflow
        .nodes
        .iter()
        .filter(|n| n.invariant)
        .map(|n| &n.id)
        .collect();

    for (mode_name, spec) in modes {
        let yunta_core::ModeInclude::Nodes(included_ids) = &spec.include else {
            continue; // `all` — every invariant below is vacuously true
        };
        for id in included_ids {
            if !top_level_ids.contains(id) {
                errors.push(CheckError::ModeReferencesUnknownNode {
                    mode: mode_name.clone(),
                    node: id.clone(),
                });
            }
        }

        let included: HashSet<&NodeId> = included_ids.iter().collect();
        for invariant_id in &invariant_ids {
            if !included.contains(invariant_id) {
                errors.push(CheckError::InvariantNodeExcludedFromMode {
                    node: (*invariant_id).clone(),
                    mode: mode_name.clone(),
                });
            }
        }

        for node in workflow.nodes.iter().filter(|n| included.contains(&n.id)) {
            if let Some(on_failure) = &node.on_failure {
                if !included.contains(&on_failure.goto) {
                    errors.push(CheckError::RerouteTargetExcludedFromMode {
                        mode: mode_name.clone(),
                        node: node.id.clone(),
                        goto: on_failure.goto.clone(),
                    });
                }
            }
            // A gate's `on:` target is the same broken-reference class
            // as a re-route's — a node excluded from the mode by
            // either its `goto` or a gate option is caught the same way.
            if let NodeKind::Gate { on, .. } = &node.kind {
                for target in on.values() {
                    if !included.contains(target) {
                        errors.push(CheckError::RerouteTargetExcludedFromMode {
                            mode: mode_name.clone(),
                            node: node.id.clone(),
                            goto: target.clone(),
                        });
                    }
                }
            }
        }
    }
}

//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Scans every literal `bash`/hook command for a `git push`
/// aimed at the base branch — the `{{project.base_branch}}` template,
/// or the configured literal name as its own token (whitespace/refspec
/// boundaries, so a branch named `main` never matches `domain`) — and
/// warns unless a gate sits somewhere before the node in the DAG
/// (transitive `depends_on`; a `parallel` child inherits its group's
/// ancestry). Literal text only, same stance as `check_commands`: a
/// command assembled at runtime is the runtime moment's problem.
pub(crate) fn collect_push_to_base_warnings(
    workflow: &Workflow,
    config: &ConfigLayer,
    warnings: &mut Vec<CheckWarning>,
) {
    let base_branch = config
        .project
        .as_ref()
        .and_then(|project| project.base_branch.as_deref());
    let pushes_to_base = |command: &str| -> Option<String> {
        if !command.contains("git push") {
            return None;
        }
        if command.contains("{{project.base_branch}}") {
            return Some(
                base_branch
                    .map(str::to_string)
                    .unwrap_or_else(|| "{{project.base_branch}}".to_string()),
            );
        }
        let base = base_branch?;
        let named = command
            .split_whitespace()
            .flat_map(|token| token.split(':'))
            .any(|token| token == base);
        named.then(|| base.to_string())
    };

    // Which top-level nodes have a gate somewhere in their transitive
    // `depends_on` ancestry.
    let nodes = &workflow.nodes;
    let index_of: HashMap<&NodeId, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (&node.id, i))
        .collect();
    fn gate_protected(
        i: usize,
        nodes: &[Node],
        index_of: &HashMap<&NodeId, usize>,
        cache: &mut Vec<Option<bool>>,
    ) -> bool {
        if let Some(known) = cache[i] {
            return known;
        }
        cache[i] = Some(false); // cycle guard; real cycles error elsewhere
        let protected = nodes[i].depends_on.iter().any(|dep| {
            index_of.get(dep).is_some_and(|&d| {
                matches!(nodes[d].kind, NodeKind::Gate { .. })
                    || gate_protected(d, nodes, index_of, cache)
            })
        });
        cache[i] = Some(protected);
        protected
    }
    let mut cache: Vec<Option<bool>> = vec![None; nodes.len()];

    for (i, node) in nodes.iter().enumerate() {
        let protected = gate_protected(i, nodes, &index_of, &mut cache);
        // A parallel child's commands push from the same ancestry as
        // its group.
        let mut targets: Vec<(&Node, &str)> = Vec::new();
        fn collect_commands<'a>(node: &'a Node, targets: &mut Vec<(&'a Node, &'a str)>) {
            if let NodeKind::Bash { run } = &node.kind {
                targets.push((node, run));
            }
            if let Some(hooks) = &node.hooks {
                for step in hooks.before.iter().chain(&hooks.after) {
                    targets.push((node, &step.run));
                }
            }
            if let NodeKind::Parallel {
                nodes: children, ..
            } = &node.kind
            {
                for child in children {
                    collect_commands(child, targets);
                }
            }
        }
        collect_commands(node, &mut targets);
        for (owner, command) in targets {
            if let Some(branch) = pushes_to_base(command) {
                if !protected {
                    warnings.push(CheckWarning::PushToBaseWithoutGate {
                        node: owner.id.clone(),
                        branch,
                    });
                }
            }
        }
    }
}

/// A `kind: gate` with `external:` needs
/// `forge.github` configured (`external.kind` is a closed enum with one
/// variant today, so this is a total match); an internal gate's own
/// `on:` mapping must reference declared options and existing targets —
/// the same broken-reference class `BrokenReference` already catches.
pub(crate) fn check_gate(
    node: &Node,
    known_ids: &HashSet<NodeId>,
    config: &yunta_core::ConfigLayer,
    errors: &mut Vec<CheckError>,
) {
    let NodeKind::Gate {
        options,
        on,
        external,
        ..
    } = &node.kind
    else {
        return;
    };
    if let Some(external) = external {
        match external.kind {
            yunta_core::ForgeKind::PullRequest => {
                let configured = config
                    .forge
                    .as_ref()
                    .is_some_and(|forge| forge.github.is_some());
                if !configured {
                    errors.push(CheckError::ExternalGateWithoutForge {
                        node: node.id.clone(),
                    });
                }
            }
        }
    }
    for (option, target) in on {
        if !options.iter().any(|declared| declared == option) {
            errors.push(CheckError::GateOnUndeclaredOption {
                node: node.id.clone(),
                option: option.clone(),
            });
        }
        if !known_ids.contains(target) {
            errors.push(CheckError::BrokenReference {
                node: node.id.clone(),
                field: format!("on.{option}"),
                target: target.clone(),
            });
        }
    }
}

/// A `parallel` group's children share a worktree
/// and join semantics a forge round-trip has no defined relationship to
/// — refused outright rather than guessing one.
pub(crate) fn check_no_gate_in_parallel(
    nodes: &[Node],
    parent_group: Option<&Node>,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        if let Some(group) = parent_group {
            if matches!(node.kind, NodeKind::Gate { .. }) {
                errors.push(CheckError::GateInsideParallel {
                    node: node.id.clone(),
                    group: group.id.clone(),
                });
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_no_gate_in_parallel(children, Some(node), errors);
        }
    }
}

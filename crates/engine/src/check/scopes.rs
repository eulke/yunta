//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Both the error and warning fan-out checks need the same question answered: which pairs of
/// top-level nodes have no dependency path between them in either
/// direction (transitive closure of `depends_on`, with a dependency on
/// a `parallel` child counting as one on its enclosing group)?
pub(crate) fn independent_top_level_pairs(workflow: &Workflow) -> Vec<(usize, usize)> {
    let nodes = &workflow.nodes;
    // Any id (child of a group included) → the top-level index it
    // belongs to.
    let mut owner: std::collections::HashMap<&NodeId, usize> = std::collections::HashMap::new();
    fn claim<'a>(
        node: &'a Node,
        top: usize,
        owner: &mut std::collections::HashMap<&'a NodeId, usize>,
    ) {
        owner.insert(&node.id, top);
        if let NodeKind::Parallel { nodes, .. } = &node.kind {
            for child in nodes {
                claim(child, top, owner);
            }
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        claim(node, i, &mut owner);
    }

    // reachable[i] = every top-level index i transitively depends on.
    let mut reachable: Vec<std::collections::HashSet<usize>> =
        vec![Default::default(); nodes.len()];
    fn walk(
        i: usize,
        nodes: &[Node],
        owner: &std::collections::HashMap<&NodeId, usize>,
        reachable: &mut [std::collections::HashSet<usize>],
        visiting: &mut [bool],
    ) {
        let resolved = reachable.get(i).is_some_and(|set| !set.is_empty());
        if visiting.get(i).copied().unwrap_or(false) || resolved {
            return;
        }
        if let Some(flag) = visiting.get_mut(i) {
            *flag = true;
        }
        let deps: Vec<usize> = nodes
            .get(i)
            .map(|node| {
                node.depends_on
                    .iter()
                    .filter_map(|dep| owner.get(dep).copied())
                    .collect()
            })
            .unwrap_or_default();
        for dep in deps {
            if dep == i {
                continue;
            }
            walk(dep, nodes, owner, reachable, visiting);
            let transitively: Vec<usize> = reachable
                .get(dep)
                .map(|set| set.iter().copied().collect())
                .unwrap_or_default();
            if let Some(set) = reachable.get_mut(i) {
                set.insert(dep);
                set.extend(transitively);
            }
        }
        if let Some(flag) = visiting.get_mut(i) {
            *flag = false;
        }
    }
    let mut visiting = vec![false; nodes.len()];
    for i in 0..nodes.len() {
        walk(i, nodes, &owner, &mut reachable, &mut visiting);
    }

    let mut pairs = Vec::new();
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let i_reaches_j = reachable.get(i).is_some_and(|set| set.contains(&j));
            let j_reaches_i = reachable.get(j).is_some_and(|set| set.contains(&i));
            if !i_reaches_j && !j_reaches_i {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// A top-level node that can write the shared worktree (same rule as
/// `parallel`'s children): anything not declared `read-only`.
pub(crate) fn writes(node: &Node) -> bool {
    node.permissions != Some(yunta_core::NodePermissions::ReadOnly)
}

/// The error half: overlapping *declared* scope on an unordered
/// pair — verifiable in advance, so an error, same rank as `parallel`.
pub(crate) fn check_fanout_scopes(
    workflow: &Workflow,
    config: &ConfigLayer,
    errors: &mut Vec<CheckError>,
) {
    if config.resolved_max_parallel_nodes() <= 1 {
        // Sequential scheduling: successive writes to one worktree are
        // legitimate, there is no concurrency to collide under.
        return;
    }
    for (i, j) in independent_top_level_pairs(workflow) {
        let (Some(a), Some(b)) = (workflow.nodes.get(i), workflow.nodes.get(j)) else {
            continue;
        };
        if !(writes(a) && writes(b)) {
            continue;
        }
        for glob_a in &a.scope {
            for glob_b in &b.scope {
                if globs_might_overlap(glob_a, glob_b) {
                    errors.push(CheckError::OverlappingFanOutScope {
                        a: a.id.clone(),
                        b: b.id.clone(),
                        glob_a: glob_a.clone(),
                        glob_b: glob_b.clone(),
                    });
                }
            }
        }
    }
}

/// The warning half: connected components of mutually-independent,
/// write-capable, scope-less top-level nodes — one warning per
/// component, members named.
pub(crate) fn collect_fanout_warnings(
    workflow: &Workflow,
    config: &ConfigLayer,
    warnings: &mut Vec<CheckWarning>,
) {
    if config.resolved_max_parallel_nodes() <= 1 {
        return;
    }
    let nodes = &workflow.nodes;
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let eligible: Vec<bool> = nodes
        .iter()
        .map(|node| writes(node) && node.scope.is_empty())
        .collect();
    for (i, j) in independent_top_level_pairs(workflow) {
        let both_eligible =
            eligible.get(i).copied().unwrap_or(false) && eligible.get(j).copied().unwrap_or(false);
        if both_eligible {
            if let Some(row) = adjacency.get_mut(i) {
                row.push(j);
            }
            if let Some(row) = adjacency.get_mut(j) {
                row.push(i);
            }
        }
    }
    let mut seen = vec![false; nodes.len()];
    for start in 0..nodes.len() {
        let no_edges = adjacency.get(start).is_none_or(Vec::is_empty);
        if seen.get(start).copied().unwrap_or(false) || no_edges {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            if seen.get(i).copied().unwrap_or(false) {
                continue;
            }
            if let Some(flag) = seen.get_mut(i) {
                *flag = true;
            }
            if let Some(node) = nodes.get(i) {
                component.push(node.id.clone());
            }
            if let Some(row) = adjacency.get(i) {
                stack.extend(row.iter().copied());
            }
        }
        component.sort();
        warnings.push(CheckWarning::UndeclaredFanOutScope {
            nodes: component
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
}

/// One `parallel` group's scope-collision status: every pair of
/// children whose declared scopes might overlap — computed in one place
/// so `check`'s error and `check_warnings`' warning can never disagree
/// about what overlaps.
pub(crate) struct GroupScope<'a> {
    overlaps: Vec<(&'a Node, &'a Node, &'a str, &'a str)>,
}

pub(crate) fn evaluate_group_scope(children: &[Node]) -> GroupScope<'_> {
    let mut overlaps = Vec::new();
    for i in 0..children.len() {
        for j in (i + 1)..children.len() {
            let (Some(a), Some(b)) = (children.get(i), children.get(j)) else {
                continue;
            };
            for glob_a in &a.scope {
                for glob_b in &b.scope {
                    if globs_might_overlap(glob_a, glob_b) {
                        overlaps.push((a, b, glob_a.as_str(), glob_b.as_str()));
                    }
                }
            }
        }
    }
    GroupScope { overlaps }
}

/// Static half of the runtime permissions rule: every literal command in
/// the workflow — bash `run`, hook steps — against the merged model,
/// parallel children included. Criteria live in the runtime ledger and
/// executors resolve through config, so both are runtime-moment
/// territory.
pub(crate) fn check_commands(
    nodes: &[Node],
    permissions: &yunta_core::PermissionsConfig,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        let mut commands: Vec<&str> = Vec::new();
        if let NodeKind::Bash { run } = &node.kind {
            commands.push(run);
        }
        if let Some(hooks) = &node.hooks {
            commands.extend(
                hooks
                    .before
                    .iter()
                    .chain(&hooks.after)
                    .map(|s| s.run.as_str()),
            );
        }
        for command in commands {
            if let Some(rule) = crate::permissions::command_violation(command, Some(permissions)) {
                errors.push(CheckError::CommandDenied {
                    node: node.id.clone(),
                    rule,
                });
            }
        }
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            check_commands(children, permissions, errors);
        }
    }
}

pub(crate) fn check_parallel_scopes(nodes: &[Node], errors: &mut Vec<CheckError>) {
    for node in nodes {
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            let group = evaluate_group_scope(children);
            for (a, b, glob_a, glob_b) in group.overlaps {
                errors.push(CheckError::OverlappingParallelScope {
                    group: node.id.clone(),
                    a: a.id.clone(),
                    b: b.id.clone(),
                    glob_a: glob_a.to_string(),
                    glob_b: glob_b.to_string(),
                });
            }
            check_parallel_scopes(children, errors);
        }
    }
}

pub(crate) fn collect_parallel_warnings(nodes: &[Node], warnings: &mut Vec<CheckWarning>) {
    for node in nodes {
        if let NodeKind::Parallel {
            nodes: children, ..
        } = &node.kind
        {
            // Only write-capable children can collide: a child
            // declaring `permissions: read-only` is out by declaration.
            let writers: Vec<&Node> = children
                .iter()
                .filter(|child| child.permissions != Some(yunta_core::NodePermissions::ReadOnly))
                .collect();
            if writers.len() >= 2 {
                let group = evaluate_group_scope(children);
                let all_writers_declared = writers.iter().all(|child| !child.scope.is_empty());
                if group.overlaps.is_empty() && !all_writers_declared {
                    warnings.push(CheckWarning::UndeclaredParallelScope {
                        group: node.id.clone(),
                    });
                }
            }
            collect_parallel_warnings(children, warnings);
        }
    }
}

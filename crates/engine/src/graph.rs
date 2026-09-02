//! Cycle detection over a `depends_on` graph — the one deterministic
//! three-color DFS the ledger's task graph and `check`'s node graph both
//! walk, so the two can never disagree about what counts as a cycle.

use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
enum Color {
    White,
    /// Carries its own index in `stack`, so finding a gray node's position
    /// never needs a fallible search.
    Gray(usize),
    Black,
}

/// Finds one cycle in a dependency graph: `adjacency[id]` lists the ids `id`
/// points to. Returns the ids on a cycle — visit order with the closing id
/// repeated — or `None` when the graph is acyclic. A target absent from
/// `adjacency` is skipped: an unknown reference is the caller's to report,
/// never a cycle. Deterministic — roots are visited in the `BTreeMap`'s
/// sorted key order and edges in their declared order, so one graph always
/// yields the same cycle.
pub fn find_cycle<T: Clone + Ord>(adjacency: &BTreeMap<T, Vec<T>>) -> Option<Vec<T>> {
    let mut color: BTreeMap<T, Color> = adjacency
        .keys()
        .cloned()
        .map(|id| (id, Color::White))
        .collect();
    let mut stack: Vec<T> = Vec::new();

    for id in adjacency.keys() {
        if matches!(color.get(id), Some(Color::White)) {
            if let Some(cycle) = visit(id, adjacency, &mut color, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
}

fn visit<T: Clone + Ord>(
    id: &T,
    adjacency: &BTreeMap<T, Vec<T>>,
    color: &mut BTreeMap<T, Color>,
    stack: &mut Vec<T>,
) -> Option<Vec<T>> {
    color.insert(id.clone(), Color::Gray(stack.len()));
    stack.push(id.clone());

    if let Some(deps) = adjacency.get(id) {
        for dep in deps {
            if !adjacency.contains_key(dep) {
                continue; // unknown target — reported separately
            }
            match color.get(dep).copied() {
                Some(Color::Gray(pos)) => {
                    let mut cycle = stack[pos..].to_vec();
                    cycle.push(dep.clone());
                    return Some(cycle);
                }
                Some(Color::Black) => continue,
                _ => {
                    if let Some(cycle) = visit(dep, adjacency, color, stack) {
                        return Some(cycle);
                    }
                }
            }
        }
    }

    stack.pop();
    color.insert(id.clone(), Color::Black);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        edges
            .iter()
            .map(|(id, deps)| {
                (
                    (*id).to_string(),
                    deps.iter().map(|d| (*d).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn an_acyclic_graph_has_no_cycle() {
        assert_eq!(
            find_cycle(&graph(&[("a", &["b"]), ("b", &["c"]), ("c", &[])])),
            None
        );
    }

    #[test]
    fn a_cycle_is_reported_with_its_closing_id_repeated() {
        let cycle = find_cycle(&graph(&[("a", &["b"]), ("b", &["a"])])).expect("a↔b is a cycle");
        assert_eq!(cycle.first(), cycle.last());
        assert!(cycle.contains(&"a".to_string()) && cycle.contains(&"b".to_string()));
    }

    #[test]
    fn an_unknown_target_is_not_a_cycle() {
        assert_eq!(find_cycle(&graph(&[("a", &["missing"])])), None);
    }

    #[test]
    fn the_same_graph_always_yields_the_same_cycle() {
        let g = graph(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"]), ("z", &["a"])]);
        let first = find_cycle(&g);
        for _ in 0..8 {
            assert_eq!(find_cycle(&g), first);
        }
    }
}

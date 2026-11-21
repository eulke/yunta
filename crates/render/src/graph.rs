//! A workflow's graph, derived from its declaration alone: `depends_on` edges,
//! `on_failure.goto` reroute edges drawn apart from them (dashed), each `parallel`
//! group drawn with its children inside it. As Mermaid and as Graphviz DOT, every
//! edge and, for a run's graph, every node labelled with what the run's log derives
//! for it; and as the diagram a page draws, each node a box holding its id alone and
//! the edges no chain of others already makes ([`drawn`]).

use std::collections::{HashMap, HashSet};

use yunta_core::{Node, NodeId, NodeKind, Workflow};
use yunta_replay::frame::RunFrame;

/// What a run's log derives for each node of its workflow, written inside the node
/// in the Mermaid and the DOT of the run's graph. Not the node's page, which says all
/// of it, nor the diagram a page draws, whose boxes hold ids alone.
pub struct Labels(HashMap<NodeId, String>);

impl Labels {
    /// One label per node of `frame`, worded by [`crate::state::NodeDisplay`], where
    /// `status` and every other surface take the same words from.
    ///
    /// Every node gets one, not only the ones the log mentions: a diagram that leaves
    /// a node bare says nothing about whether the run has yet to reach it or is never
    /// going to. The frame is where they come from, so the diagram, the run's page and
    /// the document say the same word about the same node, a node this run's mode
    /// leaves out included, and labelled `skipped`.
    pub fn of(frame: &RunFrame) -> Labels {
        Labels(
            frame
                .nodes
                .iter()
                .map(|node| {
                    (
                        node.id.clone(),
                        crate::state::NodeDisplay::from(node).label(),
                    )
                })
                .collect(),
        )
    }
}

/// The workflow's graph as a Mermaid `graph TD`: one declaration per node, labelled
/// with what `labels` says of it when there are labels, then `depends_on` edges as
/// plain arrows and `on_failure.goto` edges as dashed arrows; the two never look
/// alike.
pub fn mermaid(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("graph TD\n");

    for node in &workflow.nodes {
        let declared = |node: &Node, indent: &str| {
            format!(
                "{indent}{}[\"{}\"]\n",
                node.id,
                surco::text::escape_mermaid(&labelled(node, labels))
            )
        };
        match children_of(node) {
            // A group and its children are one unit, and a diagram that
            // drew them side by side would say they are not.
            Some(children) => {
                out.push_str(&format!(
                    "  subgraph {}[\"{}\"]\n",
                    node.id,
                    surco::text::escape_mermaid(&labelled(node, labels))
                ));
                for child in children {
                    out.push_str(&declared(child, "    "));
                }
                out.push_str("  end\n");
            }
            None => out.push_str(&declared(node, "  ")),
        }
    }

    for node in workflow.iter_nodes() {
        for dep in &node.depends_on {
            out.push_str(&format!("  {dep} --> {}\n", node.id));
        }
    }

    for node in workflow.iter_nodes() {
        if let Some(on_failure) = &node.on_failure {
            out.push_str(&format!("  {} -.-> {}\n", node.id, on_failure.goto));
        }
    }

    out
}

/// The workflow's graph as a page draws it: a Mermaid `graph TD` whose every node is
/// named by its place in the workflow and labelled with its id alone, each group with
/// its children inside it, the `depends_on` edges no chain of others already makes, and
/// the `on_failure.goto` edges dashed. A node named by its place is read whatever its id
/// holds: a fan-out's `review@reviewer` is no name a diagram reads. An edge a node reads
/// along, which a chain of other nodes already orders it behind, would cross the chain it
/// adds nothing to, so the drawing leaves it to [`mermaid`] and [`dot`], which print
/// every edge. Not [`mermaid`], which names each node by its id for a reader of the
/// source.
pub fn drawn(workflow: &Workflow) -> String {
    let places: HashMap<&NodeId, usize> = workflow
        .iter_nodes()
        .enumerate()
        .map(|(at, node)| (&node.id, at))
        .collect();
    let named = |id: &NodeId| {
        places
            .get(id)
            .map_or_else(|| id.to_string(), |at| format!("n{at}"))
    };
    let declared = |node: &Node, indent: &str| {
        format!(
            "{indent}{}[\"{}\"]\n",
            named(&node.id),
            surco::text::escape_mermaid(node.id.as_str())
        )
    };
    let mut out = String::from("graph TD\n");
    for node in &workflow.nodes {
        match children_of(node) {
            Some(children) => {
                out.push_str(&format!(
                    "  subgraph {}[\"{}\"]\n",
                    named(&node.id),
                    surco::text::escape_mermaid(node.id.as_str())
                ));
                for child in children {
                    out.push_str(&declared(child, "    "));
                }
                out.push_str("  end\n");
            }
            None => out.push_str(&declared(node, "  ")),
        }
    }
    let ordered = Ordered::of(workflow);
    for node in workflow.iter_nodes() {
        for dep in ordered.drawn_dependencies(node) {
            out.push_str(&format!("  {} --> {}\n", named(dep), named(&node.id)));
        }
    }
    for node in workflow.iter_nodes() {
        if let Some(on_failure) = &node.on_failure {
            out.push_str(&format!(
                "  {} -.-> {}\n",
                named(&node.id),
                named(&on_failure.goto)
            ));
        }
    }
    out
}

/// Every node a workflow's `depends_on` orders each node behind, near or far.
struct Ordered<'w>(HashMap<&'w NodeId, HashSet<&'w NodeId>>);

impl<'w> Ordered<'w> {
    /// What each node of `workflow` is ordered behind.
    fn of(workflow: &'w Workflow) -> Self {
        let nodes: HashMap<&NodeId, &Node> =
            workflow.iter_nodes().map(|node| (&node.id, node)).collect();
        let mut behind = HashMap::new();
        for node in workflow.iter_nodes() {
            behind_of(&node.id, &nodes, &mut behind);
        }
        Ordered(behind)
    }

    /// The nodes `node` depends on that no other of them is already ordered behind.
    fn drawn_dependencies<'n>(&self, node: &'n Node) -> Vec<&'n NodeId> {
        let deps = &node.depends_on;
        deps.iter()
            .filter(|dep| {
                !deps.iter().any(|other| {
                    other != *dep
                        && self
                            .0
                            .get(other)
                            .is_some_and(|before| before.contains(*dep))
                })
            })
            .collect()
    }
}

/// What `id` is ordered behind, near or far, worked out once and kept in `behind`. A
/// dependency on no node of the workflow orders nothing behind it.
fn behind_of<'w>(
    id: &'w NodeId,
    nodes: &HashMap<&'w NodeId, &'w Node>,
    behind: &mut HashMap<&'w NodeId, HashSet<&'w NodeId>>,
) -> HashSet<&'w NodeId> {
    if let Some(known) = behind.get(id) {
        return known.clone();
    }
    // Kept empty while it is worked out, so a cycle the check refuses ends here.
    behind.insert(id, HashSet::new());
    let mut before = HashSet::new();
    for dep in nodes
        .get(id)
        .map_or(&[][..], |node| node.depends_on.as_slice())
    {
        before.insert(dep);
        before.extend(behind_of(dep, nodes, behind));
    }
    behind.insert(id, before.clone());
    before
}

/// The same graph as Graphviz DOT: `depends_on` edges solid, `on_failure.goto`
/// edges dashed, node labels quoted.
pub fn dot(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("digraph workflow {\n  rankdir=TB;\n");
    let declared = |node: &Node, indent: &str| {
        format!(
            "{indent}\"{}\" [label=\"{}\"];\n",
            escape_dot(node.id.as_str()),
            escape_dot(&labelled(node, labels))
        )
    };
    for node in &workflow.nodes {
        match children_of(node) {
            Some(children) => {
                out.push_str(&format!(
                    "  subgraph cluster_{} {{\n",
                    escape_dot(node.id.as_str())
                ));
                out.push_str(&format!(
                    "    label=\"{}\";\n",
                    escape_dot(&labelled(node, labels))
                ));
                for child in children {
                    out.push_str(&declared(child, "    "));
                }
                out.push_str("  }\n");
            }
            None => out.push_str(&declared(node, "  ")),
        }
    }
    for node in workflow.iter_nodes() {
        for dep in &node.depends_on {
            out.push_str(&format!(
                "  \"{}\" -> \"{}\";\n",
                escape_dot(dep.as_str()),
                escape_dot(node.id.as_str())
            ));
        }
    }
    for node in workflow.iter_nodes() {
        if let Some(on_failure) = &node.on_failure {
            out.push_str(&format!(
                "  \"{}\" -> \"{}\" [style=dashed];\n",
                escape_dot(node.id.as_str()),
                escape_dot(on_failure.goto.as_str())
            ));
        }
    }
    out.push_str("}\n");
    out
}

/// A `parallel` group's children, `None` for every other node.
fn children_of(node: &Node) -> Option<&[Node]> {
    match &node.kind {
        NodeKind::Parallel { nodes, .. } => Some(nodes),
        _ => None,
    }
}

/// What the graph writes inside a node: its id, and the state a run's log derives
/// for it when there is a run.
fn labelled(node: &Node, labels: Option<&Labels>) -> String {
    match labels.and_then(|labels| labels.0.get(&node.id)) {
        Some(state) => format!("{}: {}", node.id, state),
        None => node.id.to_string(),
    }
}

/// Escapes a DOT quoted-string label: backslash and double quote are the two
/// characters DOT reads specially inside `"..."`.
fn escape_dot(text: &str) -> String {
    let text = surco::text::one_line(text);
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{drawn, escape_dot, mermaid};
    use surco::text::escape_mermaid;
    use yunta_core::Workflow;

    /// A node that reads what a fan-out wrote, ordered behind it twice: once by the
    /// chain through `review@a`, once by the edge its read adds.
    const READS_ALONG: &str = r#"
name: reads
nodes:
  - { id: brief, kind: bash, run: "true" }
  - { id: review@a, kind: bash, run: "true", depends_on: [brief] }
  - { id: fix, kind: bash, run: "true", depends_on: [review@a, brief] }
"#;

    /// The drawing names each node by its place and labels it with its id, and leaves
    /// out the edge a chain of others already makes, which the printed source keeps.
    #[test]
    fn the_drawing_names_each_node_by_its_place_and_leaves_an_edge_a_chain_makes() {
        let workflow = Workflow::parse(READS_ALONG).expect("the workflow parses");
        let source = drawn(&workflow);
        assert!(source.contains("  n1[\"review@a\"]\n"), "{source}");
        assert!(source.contains("  n0 --> n1\n  n1 --> n2\n"), "{source}");
        assert!(!source.contains("n0 --> n2"), "{source}");
        assert!(
            mermaid(&workflow, None).contains("  brief --> fix\n"),
            "the printed source keeps every edge"
        );
    }

    #[test]
    fn mermaid_escaping_covers_every_html_significant_character() {
        assert_eq!(escape_mermaid("a\"b<c>d&e"), "a&quot;b&lt;c&gt;d&amp;e");
    }

    #[test]
    fn mermaid_escaping_collapses_newlines_onto_one_line() {
        let escaped = escape_mermaid("first\nsecond\r\nthird");
        assert!(
            !escaped.contains('\n') && !escaped.contains('\r'),
            "got: {escaped}"
        );
        assert!(
            escaped.contains("first") && escaped.contains("third"),
            "got: {escaped}"
        );
    }

    #[test]
    fn dot_escaping_covers_quote_and_backslash_and_newlines() {
        assert_eq!(escape_dot(r#"a\b"c"#), r#"a\\b\"c"#);
        let escaped = escape_dot("first\nsecond");
        assert!(!escaped.contains('\n'), "got: {escaped}");
    }
}

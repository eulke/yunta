//! A workflow's graph, derived from its declaration alone: `depends_on` edges,
//! `on_failure.goto` reroute edges drawn apart from them (dashed), each `parallel`
//! group drawn with its children inside it, and, for a run's graph, every node
//! labelled with what the run's log derives for it. As Mermaid, which a page draws
//! as a diagram, and as Graphviz DOT.

use std::collections::HashMap;

use yunta_core::{Node, NodeId, NodeKind, Workflow};
use yunta_replay::frame::RunFrame;

/// What a run's log derives for each node of its workflow, written inside the node
/// on the run's graph. Not the node's page, which says all of it.
pub struct Labels(HashMap<NodeId, String>);

impl Labels {
    /// One label per node of `frame`, worded by [`crate::state::NodeDisplay`], where
    /// `status` and every other surface take the same words from.
    ///
    /// Every node gets one, not only the ones the log mentions: a diagram that leaves
    /// a node bare says nothing about whether the run has yet to reach it or is never
    /// going to. The frame is where they come from, so the diagram, the page and the
    /// document say the same word about the same node, a node this run's mode leaves
    /// out included, and labelled `skipped`.
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
    use super::escape_dot;
    use surco::text::escape_mermaid;

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

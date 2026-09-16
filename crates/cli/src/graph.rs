//! `yunta graph <workflow> | --run <id> [--format mermaid|dot]`: pure
//! derivation of the DAG — `depends_on` edges, `on_failure.goto`
//! re-route edges visually differentiated (dashed) from them, each
//! `parallel` group drawn with its children inside it, and, given
//! `--run`, every node annotated with the state that run's event log
//! derives for it (`yunta_engine::derive`). No agent involved in
//! producing the graph itself, same shape as `status`.
//!
//! Exactly one source (D179): a path or a catalog name reads the
//! workflow off disk, `--run` draws the one that run froze into its
//! manifest. Naming both is refused — a run's diagram is of the run,
//! and the file beside it may say something else by now.

use std::collections::HashMap;
use std::path::Path;

use yunta_core::{Clock, NodeId, RunId, Workflow};

use crate::commands::{check_or_refuse, resolve_workflow_ref};
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::NodeDisplay;
use yunta_engine::RunFrame;

type Labels = HashMap<NodeId, String>;

/// The diagram languages `--format` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GraphFormat {
    Mermaid,
    Dot,
}

pub async fn graph(
    workflow_path: Option<&Path>,
    run_id: Option<&RunId>,
    format: GraphFormat,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;

    let (workflow, labels) = match (workflow_path, run_id) {
        (Some(path), None) => {
            // A bare catalog name resolves the same way `check` and
            // `run` resolve it — `graph review` works without spelling
            // out the path.
            let path = resolve_workflow_ref(&ctx.cwd, path)?;
            let workflow = crate::load_workflow(&path)?;
            check_or_refuse(&ctx.cwd, &workflow, &ctx.project.config, &path)?;
            (workflow, None)
        }
        (None, Some(run_id)) => {
            let open = ctx.open_run(run_id).await?;
            let manifest = open.manifest.doc;
            let frame = crate::commands::status::progress::frame(
                run_id,
                &manifest,
                &open.events,
                ctx.clock.now(),
            );
            (manifest.workflow, derived_labels(&frame))
        }
        (Some(_), Some(_)) => {
            return Err(CliError::msg(
                "`graph` draws one workflow: a path or a catalog name reads it off disk, \
                 `--run <id>` draws the one that run froze. Drop one of the two.",
            ))
        }
        (None, None) => {
            return Err(CliError::msg(
                "`graph` needs a workflow: name one, or pass `--run <id>` to draw the one \
                 that run froze.",
            ))
        }
    };

    let rendered = match format {
        GraphFormat::Mermaid => render_mermaid(&workflow, labels.as_ref()),
        GraphFormat::Dot => render_dot(&workflow, labels.as_ref()),
    };
    print!("{rendered}");
    Ok(Outcome::Success)
}

/// One label per node of the graph, derived from the event log
/// (`yunta_engine::derive`) and worded by `crate::render`, which is where
/// `status` and every other surface take the same words from.
///
/// Every node gets one, not only the ones the log mentions: a diagram
/// that leaves a node bare says nothing about whether the run has yet
/// to reach it or is never going to. The frame is where they come
/// from, so the diagram, the page and the document say the same word
/// about the same node — a node this run's mode leaves out included,
/// and labelled `skipped`.
fn derived_labels(frame: &RunFrame) -> Option<Labels> {
    Some(
        frame
            .nodes
            .iter()
            .map(|node| (node.id.clone(), NodeDisplay::standing(&node.state).label()))
            .collect(),
    )
}

/// Renders the workflow's DAG as a Mermaid `graph TD`: one declaration per
/// node (labeled with its derived state when `labels` is given), then
/// `depends_on` edges as plain arrows and `on_failure.goto` edges as
/// dashed arrows — the two must never look alike.
fn render_mermaid(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("graph TD\n");

    for node in &workflow.nodes {
        let declared = |node: &yunta_core::Node, indent: &str| {
            format!(
                "{indent}{}[\"{}\"]\n",
                node.id,
                escape_mermaid(&labelled(node, labels))
            )
        };
        match children_of(node) {
            // A group and its children are one unit, and a diagram that
            // drew them side by side would say they are not.
            Some(children) => {
                out.push_str(&format!(
                    "  subgraph {}[\"{}\"]\n",
                    node.id,
                    escape_mermaid(&labelled(node, labels))
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

/// A `parallel` group's children, `None` for every other node.
fn children_of(node: &yunta_core::Node) -> Option<&[yunta_core::Node]> {
    match &node.kind {
        yunta_core::NodeKind::Parallel { nodes, .. } => Some(nodes),
        _ => None,
    }
}

/// What the diagram writes inside a node: its id, and the state a run's
/// log derives for it when there is a run.
fn labelled(node: &yunta_core::Node, labels: Option<&Labels>) -> String {
    match labels.and_then(|labels| labels.get(&node.id)) {
        Some(state) => format!("{}: {}", node.id, state),
        None => node.id.to_string(),
    }
}

/// The same DAG as Graphviz DOT: `depends_on` edges solid,
/// `on_failure.goto` edges dashed, node labels quoted.
fn render_dot(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("digraph workflow {\n  rankdir=TB;\n");
    let declared = |node: &yunta_core::Node, indent: &str| {
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

/// Escapes a Mermaid node label. Labels sit inside `["..."]` and Mermaid
/// renders them as HTML, so every character HTML or the quoting reads
/// specially becomes an entity. `&` is handled in the same single pass as
/// the rest, so an entity this inserts is never re-escaped.
///
/// A label is one line by construction: the collapse belongs to every
/// surface with room for one line, so it lives in `yunta_core::text`
/// rather than being re-derived here and in `escape_dot`.
fn escape_mermaid(text: &str) -> String {
    let text = yunta_core::text::one_line(text);
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Escapes a DOT quoted-string label: backslash and double quote are the
/// two characters DOT reads specially inside `"..."`.
fn escape_dot(text: &str) -> String {
    let text = yunta_core::text::one_line(text);
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
    use super::{escape_dot, escape_mermaid};

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

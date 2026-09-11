//! `yunta graph <workflow> [--run <id>] [--format mermaid|dot]`: pure
//! derivation of the DAG — `depends_on` edges, `on_failure.goto`
//! re-route edges visually differentiated (dashed) from them, and,
//! given `--run`, every node annotated with the state that run's event
//! log derives for it (`yunta_engine::derive`). No agent involved in
//! producing the graph itself, same shape as `status`.

use std::collections::HashMap;
use std::path::Path;

use yunta_core::{NodeId, RunId, Workflow};
use yunta_storage::Storage;

use crate::commands::{check_or_refuse, resolve_workflow_ref};
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::NodeDisplay;
use crate::{load_yaml, project};

type Labels = HashMap<NodeId, String>;

/// The diagram languages `--format` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GraphFormat {
    Mermaid,
    Dot,
}

pub fn graph(
    workflow_path: &Path,
    run_id: Option<&RunId>,
    format: GraphFormat,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;

    // A bare catalog name resolves the same way `check` and `run` resolve
    // it — `graph review` works without spelling out the path.
    let workflow_path = resolve_workflow_ref(&ctx.cwd, workflow_path)?;
    let workflow: Workflow = load_yaml(&workflow_path, "workflow")?;

    check_or_refuse(&workflow, &ctx.project.config, &workflow_path)?;

    let labels = match run_id {
        Some(run_id) => Some(derive_labels(&ctx.project, run_id, &workflow)?),
        None => None,
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
/// that leaves a node bare says nothing about whether the run has yet to
/// reach it or is never going to. A node this run's mode excludes is
/// skipped, a node the mode includes and the log has nothing for never
/// ran, and the two are different answers to the same question.
fn derive_labels(
    project: &project::Project,
    run_id: &RunId,
    workflow: &Workflow,
) -> Result<Labels, CliError> {
    let storage = Storage::open(&project.storage_path)?;
    let events = storage.events_for_run(run_id)?;
    if events.is_empty() {
        return Err(CliError::msg(format!(
            "no run `{run_id}` in {}",
            project.storage_path.display()
        )));
    }

    let state = yunta_engine::derive(&events);
    let mode = yunta_core::events::run_mode(&events);
    let included = yunta_engine::mode_included_nodes(workflow, &mode);
    Ok(workflow
        .nodes
        .iter()
        .map(|node| {
            let display = match &included {
                Some(included) if !included.contains(&node.id) => NodeDisplay::skipped(),
                _ => NodeDisplay::of(state.nodes.get(&node.id)),
            };
            (node.id.clone(), display.label())
        })
        .collect())
}

/// Renders the workflow's DAG as a Mermaid `graph TD`: one declaration per
/// node (labeled with its derived state when `labels` is given), then
/// `depends_on` edges as plain arrows and `on_failure.goto` edges as
/// dashed arrows — the two must never look alike.
fn render_mermaid(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("graph TD\n");

    for node in &workflow.nodes {
        let label = match labels.and_then(|labels| labels.get(&node.id)) {
            Some(state) => format!("{}: {}", node.id, state),
            None => node.id.to_string(),
        };
        out.push_str(&format!("  {}[\"{}\"]\n", node.id, escape_mermaid(&label)));
    }

    for node in &workflow.nodes {
        for dep in &node.depends_on {
            out.push_str(&format!("  {dep} --> {}\n", node.id));
        }
    }

    for node in &workflow.nodes {
        if let Some(on_failure) = &node.on_failure {
            out.push_str(&format!("  {} -.-> {}\n", node.id, on_failure.goto));
        }
    }

    out
}

/// The same DAG as Graphviz DOT: `depends_on` edges solid,
/// `on_failure.goto` edges dashed, node labels quoted.
fn render_dot(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("digraph workflow {\n  rankdir=TB;\n");
    for node in &workflow.nodes {
        let text = match labels.and_then(|labels| labels.get(&node.id)) {
            Some(state) => format!("{}: {}", node.id, state),
            None => node.id.to_string(),
        };
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\"];\n",
            escape_dot(node.id.as_str()),
            escape_dot(&text)
        ));
    }
    for node in &workflow.nodes {
        for dep in &node.depends_on {
            out.push_str(&format!(
                "  \"{}\" -> \"{}\";\n",
                escape_dot(dep.as_str()),
                escape_dot(node.id.as_str())
            ));
        }
    }
    for node in &workflow.nodes {
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

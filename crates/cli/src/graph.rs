//! `yunta graph <workflow> [--run <id>] [--format mermaid|dot]`: pure
//! derivation of the DAG — `depends_on` edges, `on_failure.goto`
//! re-route edges visually differentiated (dashed) from them, and,
//! given `--run`, each node annotated with its derived state
//! (`yunta_engine::derive`). No agent involved in producing the graph
//! itself, same shape as `status`.

use std::collections::HashMap;
use std::path::Path;

use yunta_core::{NodeId, RunId, Workflow};
use yunta_engine::NodeState;
use yunta_storage::Storage;

use crate::commands::check_or_refuse;
use crate::context::Context;
use crate::error::{CliError, Outcome};
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

    let workflow: Workflow = load_yaml(workflow_path, "workflow")?;

    check_or_refuse(&workflow, &ctx.project.config, workflow_path)?;

    let labels = match run_id {
        Some(run_id) => Some(derive_labels(&ctx.project, run_id)?),
        None => None,
    };

    let rendered = match format {
        GraphFormat::Mermaid => render_mermaid(&workflow, labels.as_ref()),
        GraphFormat::Dot => render_dot(&workflow, labels.as_ref()),
    };
    print!("{rendered}");
    Ok(Outcome::Success)
}

/// Derives run state from the event log (`yunta_engine::derive`) and
/// reduces it to one display label per node — the same source
/// `status` reads, just formatted for a Mermaid node instead of a list.
fn derive_labels(project: &project::Project, run_id: &RunId) -> Result<Labels, CliError> {
    let storage = Storage::open(&project.storage_path)?;
    let events = storage.events_for_run(run_id)?;
    if events.is_empty() {
        return Err(CliError::msg(format!(
            "no run `{run_id}` in {}",
            project.storage_path.display()
        )));
    }

    let state = yunta_engine::derive(&events);
    Ok(state
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node_state_label(node)))
        .collect())
}

fn node_state_label(node: &NodeState) -> String {
    match node {
        NodeState::Running { attempt } => format!("running (attempt {attempt})"),
        NodeState::Finished { outcome, .. } => format!("finished — {outcome}"),
        NodeState::Failed { outcome, .. } => format!("failed — {outcome}"),
        // A run paused on a gate shows its waiting node distinctly —
        // with the forge handle when there is one.
        NodeState::Waiting { external_ref } => match external_ref {
            Some(external_ref) => format!("waiting — {external_ref}"),
            None => "waiting".to_string(),
        },
    }
}

/// Renders the workflow's DAG as a Mermaid `graph TD`: one declaration per
/// node (labeled with its derived state when `labels` is given), then
/// `depends_on` edges as plain arrows and `on_failure.goto` edges as
/// dashed arrows — the two must never look alike.
fn render_mermaid(workflow: &Workflow, labels: Option<&Labels>) -> String {
    let mut out = String::from("graph TD\n");

    for node in &workflow.nodes {
        let text = match labels.and_then(|labels| labels.get(&node.id)) {
            Some(state) => format!("{}: {}", node.id, escape_label(state)),
            None => node.id.to_string(),
        };
        out.push_str(&format!("  {}[\"{text}\"]\n", node.id));
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

/// Mermaid node labels are double-quoted text — escape embedded quotes so
/// an outcome message never breaks the diagram's syntax.
fn escape_label(text: &str) -> String {
    text.replace('"', "&quot;")
}

/// DOT quoted strings escape backslashes and double quotes.
fn escape_dot(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

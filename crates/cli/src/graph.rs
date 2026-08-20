//! `yunta graph <workflow> [--run <id>]` (T7.8 recorte, D75): pure
//! derivation of the DAG as Mermaid — `depends_on` edges, `on_failure.goto`
//! re-route edges visually differentiated (dashed) from them, and, given
//! `--run`, each node annotated with its derived state (§8.5,
//! `yunta_engine::derive`). No agent involved in producing the graph
//! itself, same shape as `status`.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use yunta_core::{NodeId, RunId, Workflow};
use yunta_engine::NodeState;
use yunta_storage::Storage;

use crate::commands::check_or_refuse;
use crate::{load_yaml, project};

type Labels = HashMap<NodeId, String>;

pub fn graph(workflow_path: &Path, run_id: Option<&str>) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match project::resolve(&cwd) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let workflow: Workflow = match load_yaml(workflow_path, "workflow") {
        Ok(w) => w,
        Err(code) => return code,
    };

    if let Err(code) = check_or_refuse(&workflow, &project.config) {
        return code;
    }

    let labels = match run_id {
        Some(run_id) => match derive_labels(&project, run_id) {
            Ok(labels) => Some(labels),
            Err(code) => return code,
        },
        None => None,
    };

    print!("{}", render_mermaid(&workflow, labels.as_ref()));
    ExitCode::SUCCESS
}

/// Derives run state from the event log (T2.3's `yunta_engine::derive`)
/// and reduces it to one display label per node — the same source
/// `status` reads, just formatted for a Mermaid node instead of a list.
fn derive_labels(project: &project::Project, run_id: &str) -> Result<Labels, ExitCode> {
    let storage = Storage::open(&project.storage_path).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })?;

    let run_id = RunId::from(run_id);
    let events = storage.events_for_run(&run_id).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })?;
    if events.is_empty() {
        eprintln!(
            "error: no run `{run_id}` in {}",
            project.storage_path.display()
        );
        return Err(ExitCode::FAILURE);
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
        // §8.5/D75: a run paused on a gate shows its waiting node
        // distinctly — with the forge handle when there is one.
        NodeState::Waiting { external_ref } => match external_ref {
            Some(external_ref) => format!("waiting — {external_ref}"),
            None => "waiting".to_string(),
        },
    }
}

/// Renders the workflow's DAG as a Mermaid `graph TD`: one declaration per
/// node (labeled with its derived state when `labels` is given), then
/// `depends_on` edges as plain arrows and `on_failure.goto` edges as
/// dashed arrows — the two must never look alike, per D75.
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

/// Mermaid node labels are double-quoted text — escape embedded quotes so
/// an outcome message never breaks the diagram's syntax.
fn escape_label(text: &str) -> String {
    text.replace('"', "&quot;")
}

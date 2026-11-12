//! `yunta graph <workflow> | --run <id> [--format mermaid|dot]`: the
//! workflow's graph as render's `graph` derives it, printed as Mermaid or
//! DOT, and, given `--run`, every node labelled with the state that run's
//! event log derives for it. No agent involved in producing the graph
//! itself, same shape as `status`.
//!
//! Exactly one source: a path or a catalog name reads the
//! workflow off disk, `--run` draws the one that run froze into its
//! manifest. Naming both is refused — a run's diagram is of the run,
//! and the file beside it may say something else by now.

use std::path::Path;

use yunta_core::RunId;
use yunta_render::graph::{self, Labels};

use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;

/// The diagram languages `--format` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GraphFormat {
    Mermaid,
    Dot,
}

pub async fn graph(
    shell: &Shell,
    workflow_path: Option<&Path>,
    run_id: Option<&RunId>,
    format: GraphFormat,
    look: &surco::look::Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;

    let (workflow, labels, outcome) = match (workflow_path, run_id) {
        (Some(path), None) => {
            // A bare catalog name resolves the same way `check` and
            // `run` resolve it — `graph review` works without spelling
            // out the path.
            let yunta_api::WorkflowFile { path, workflow } =
                project.workspace.read_workflow(path)?;
            // A file that reads is drawn whatever the project's config
            // makes of it, with the verdict beside the drawing, on
            // stderr, and in the exit code: the graph is the file's.
            let judged = yunta_api::Judged {
                workflow: &workflow,
                path: &path,
                config: &project.config,
                conflicts: project.layer_conflicts()?,
            };
            let reach = yunta_api::VerdictReach::Workflow;
            let verdict = project.verdict(judged, reach, &shell.stop()).await?;
            // A verdict with nothing to say stays off the drawing's way.
            if !verdict.passes() || !verdict.cautions().is_empty() {
                crate::output::show_on_stderr(look, &verdict.checked(&shell.stop()).await?);
            }
            let outcome = match verdict.passes() {
                true => Outcome::Success,
                false => Outcome::Reported,
            };
            (workflow, None, outcome)
        }
        (None, Some(run_id)) => {
            let open = project.run(run_id).await?;
            let frame = open.frame();
            let labels = Some(Labels::of(&frame));
            (open.manifest.doc.workflow, labels, Outcome::Success)
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
        GraphFormat::Mermaid => graph::mermaid(&workflow, labels.as_ref()),
        GraphFormat::Dot => graph::dot(&workflow, labels.as_ref()),
    };
    crate::output::raw(&rendered);
    Ok(outcome)
}

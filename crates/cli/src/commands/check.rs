//! `yunta check <workflow>`: static validation of a workflow — no run
//! created, no agent involved. Resolves a bare catalog name the same way
//! `run` and `graph` do, merges the project's real config layers (or a
//! single explicit `--config` file), and reports every error the engine's
//! `check` finds, its warnings, and any verification-effectiveness
//! findings from this workflow's past runs.

use std::path::Path;

use surco::look::Look;
use yunta_core::ConfigLayer;

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{blank, show};
use crate::shell::Shell;
use yunta_api::{AuditedWorkflow, Judged, Project, Stop, Verdict, VerdictReach};

pub async fn check(
    shell: &Shell,
    workflow_path: &Path,
    config_path: Option<&Path>,
    look: &Look,
) -> Result<Outcome, CliError> {
    // The one prologue: the directory this command resolves everything
    // against (the catalog reference, the config layers, the composition
    // graph) is the same project its history is read from, so a project
    // resolved twice cannot be two projects.
    let project = shell.workspace().project()?;
    let AuditedWorkflow {
        path,
        workflow,
        problems,
    } = shell.workspace().audit_workflow(workflow_path)?;
    let (config, conflicts) = config_for(&project, config_path)?;
    let judged = Judged {
        workflow: &workflow,
        path: &path,
        config: &config,
        conflicts,
    };
    let verdict = project
        .verdict(judged, VerdictReach::Workflow, &shell.stop())
        .await?
        .with_file_problems(problems.diagnostics);

    // Verification-effectiveness findings, surfaced here too — right when
    // someone is already looking at this workflow — not only via `stats
    // --workflow`. Best effort: a project with no state root yet (nothing
    // ever ran) or an unnamed workflow simply shows nothing, the same
    // stance `list_workflows` takes on missing history.
    let findings = project.history(&workflow.name).await.findings(&workflow);
    let outcome = report(&verdict, &shell.stop(), look).await?;
    // Advice after the verdict, which is what was asked for.
    if !findings.is_empty() {
        blank();
        show(look, &findings);
    }
    Ok(outcome)
}

/// The config `check` judges against, and what its layers refuse among
/// themselves: the project's own layers, merged as a run merges them —
/// a lower layer re-permitting what a higher one denied is refused,
/// citing both — or one already-merged file named with `--config`, which
/// has nothing layered to conflict.
fn config_for(
    project: &Project,
    config_path: Option<&Path>,
) -> Result<(ConfigLayer, Vec<String>), CliError> {
    Ok(match config_path {
        Some(path) => (project.workspace.read_config(path)?, Vec::new()),
        None => (project.config.clone(), project.layer_conflicts()?),
    })
}

/// The verdict as `check` reports it: its page on stdout when the workflow
/// can run here, and on stderr, each problem pointed at in its line, when it
/// cannot.
async fn report(verdict: &Verdict<'_>, stop: &Stop, look: &Look) -> Result<Outcome, CliError> {
    let checked = verdict.checked(stop).await?;
    if verdict.passes() {
        show(look, &checked);
        return Ok(Outcome::Success);
    }
    crate::output::show_on_stderr(look, &checked);
    Ok(Outcome::Reported)
}

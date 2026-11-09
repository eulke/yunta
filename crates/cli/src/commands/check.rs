//! `yunta check <workflow>`: static validation of a workflow — no run
//! created, no agent involved. Resolves a bare catalog name the same way
//! `run` and `graph` do, merges the project's real config layers (or a
//! single explicit `--config` file), and reports every error the engine's
//! `check` finds, its warnings, and any verification-effectiveness
//! findings from this workflow's past runs.

use std::path::Path;

use surco::look::Look;
use yunta_core::ConfigLayer;

use crate::context::Context;
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{blank, show};
use crate::shell::Shell;
use yunta_api::{Judged, Project, Verdict, VerdictReach};
use yunta_render::refusal::Refused;

pub async fn check(
    shell: &Shell,
    workflow_path: &Path,
    config_path: Option<&Path>,
    look: &Look,
) -> Result<Outcome, CliError> {
    // The one prologue: the directory this command resolves everything
    // against — the catalog reference, the config layers, the
    // composition graph — comes from the same `Context` its history
    // reading does, so a project resolved twice cannot be two projects.
    let ctx = Context::of(shell)?;
    let workflow_path = shell.workspace().workflow_file(workflow_path)?;
    let (workflow, in_file, text) = crate::audit_workflow(&workflow_path)?;
    let Some(workflow) = workflow else {
        return Err(Refused::Document {
            report: Box::new(in_file),
            text: Some(text),
        }
        .into());
    };

    let (config, conflicts) = config_for(&ctx.project, config_path)?;
    let judged = Judged {
        workflow: &workflow,
        path: &workflow_path,
        config: &config,
        conflicts,
    };
    let verdict = ctx
        .project
        .verdict(judged, VerdictReach::Workflow, &ctx.stop())
        .await?
        .with_file_problems(in_file.diagnostics);

    // Verification-effectiveness findings, surfaced here too — right when
    // someone is already looking at this workflow — not only via `stats
    // --workflow`. Best effort: a project with no state root yet (nothing
    // ever ran) or an unnamed workflow simply shows nothing, the same
    // stance `list_workflows` takes on missing history.
    let findings = ctx
        .project
        .history(&workflow.name)
        .await
        .findings(&workflow);
    let outcome = report(&verdict, &ctx, look).await?;
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
        Some(path) => (read_config(path)?, Vec::new()),
        None => (project.config.clone(), project.layer_conflicts()?),
    })
}

/// The verdict as `check` reports it: its page on stdout when the workflow
/// can run here, and on stderr, each problem pointed at in its line, when it
/// cannot.
async fn report(verdict: &Verdict<'_>, ctx: &Context, look: &Look) -> Result<Outcome, CliError> {
    let checked = verdict.checked(&ctx.stop()).await?;
    if verdict.passes() {
        show(look, &checked);
        return Ok(Outcome::Success);
    }
    crate::output::show_on_stderr(look, &checked);
    Ok(Outcome::Reported)
}

/// The one merged config file `--config` names, read through core's reader of
/// a config layer.
fn read_config(path: &Path) -> Result<ConfigLayer, CliError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|source| CliError::io("read config at", path.display(), source))?;
    ConfigLayer::read(&contents)
        .map_err(|e| CliError::msg(format!("failed to parse config at {}: {e}", path.display())))
}

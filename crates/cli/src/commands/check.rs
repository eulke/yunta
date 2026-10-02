//! `yunta check <workflow>`: static validation of a workflow — no run
//! created, no agent involved. Resolves a bare catalog name the same way
//! `run` and `graph` do, merges the project's real config layers (or a
//! single explicit `--config` file), and reports every error the engine's
//! `check` finds, its warnings, and any verification-effectiveness
//! findings from this workflow's past runs.

use std::path::Path;

use yunta_core::ConfigLayer;

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::{load_yaml, project};

pub async fn check(workflow_path: &Path, config_path: Option<&Path>) -> Result<Outcome, CliError> {
    // The one prologue: the directory this command resolves everything
    // against — the catalog reference, the config layers, the
    // composition graph — comes from the same `Context` its history
    // reading does, so a project resolved twice cannot be two projects.
    let ctx = Context::load()?;
    let cwd = ctx.cwd.clone();
    let workflow_path = super::resolve_workflow_ref(&cwd, workflow_path)?;
    let (workflow, in_file, text) = crate::audit_workflow(&workflow_path)?;
    let Some(workflow) = workflow else {
        return Err(CliError::Document {
            report: in_file,
            text: Some(text),
        });
    };

    let (config, conflicts) = config_for(&cwd, config_path)?;
    let verdict = super::verdict::verdict(
        &ctx,
        &workflow,
        &workflow_path,
        &config,
        conflicts,
        super::verdict::Reach::Workflow,
    )
    .await
    .with_file_problems(in_file.diagnostics);

    // Verification-effectiveness findings, surfaced here too — right when
    // someone is already looking at this workflow — not only via `stats
    // --workflow`. Best effort: a project with no state root yet (nothing
    // ever ran) or an unnamed workflow simply shows nothing, the same
    // stance `list_workflows` takes on missing history.
    let opened = super::stats::history(&ctx, &workflow.name).await;
    let (history, _) = super::stats::raw_history(&opened);
    let findings = yunta_engine::analyze_verification_effectiveness(&workflow, &history);
    let outcome = verdict.report(&ctx, workflow_path.display()).await;
    // Advice after the verdict, which is what was asked for.
    if let Some(advice) = super::stats::verification_findings(&findings) {
        let look = crate::render::stdout_look();
        print!("\n{}", crate::render::draw(advice, &look));
    }
    Ok(outcome)
}

/// The config `check` judges against, and what its layers refuse among
/// themselves: the project's own layers, merged as a run merges them —
/// a lower layer re-permitting what a higher one denied is refused,
/// citing both — or one already-merged file named with `--config`, which
/// has nothing layered to conflict.
fn config_for(
    cwd: &Path,
    config_path: Option<&Path>,
) -> Result<(ConfigLayer, Vec<String>), CliError> {
    Ok(match config_path {
        Some(path) => (load_yaml(path, "config")?, Vec::new()),
        None => (
            ConfigLayer::merge_layers(
                project::load_named_layers(cwd)?
                    .into_iter()
                    .map(|(_, layer)| layer),
            ),
            super::verdict::layer_conflicts(cwd)?,
        ),
    })
}

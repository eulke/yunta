//! `yunta check <workflow>`: static validation of a workflow — no run
//! created, no agent involved. Resolves a bare catalog name the same way
//! `run` and `graph` do, merges the project's real config layers (or a
//! single explicit `--config` file), and reports every error the engine's
//! `check` finds, its warnings, and any verification-effectiveness
//! findings from this workflow's past runs.

use std::path::Path;

use yunta_core::ConfigLayer;

use crate::context::Context;
use crate::error::{note, CliError, Outcome};
use crate::{load_yaml, project};

pub async fn check(workflow_path: &Path, config_path: Option<&Path>) -> Result<Outcome, CliError> {
    // The one prologue: the directory this command resolves everything
    // against — the catalog reference, the config layers, the
    // composition graph — comes from the same `Context` its history
    // reading does, so a project resolved twice cannot be two projects.
    let ctx = Context::load()?;
    let cwd = ctx.cwd.clone();
    let workflow_path = super::resolve_workflow_ref(&cwd, workflow_path)?;
    let workflow = crate::load_workflow(&workflow_path)?;

    // Without `--config`, check sees the project's real layers — the same
    // ones a run would — including the `permissions` layer conflict check
    // (a lower layer re-permitting what a higher one denied is refused
    // here, citing both layers). An explicit `--config` is a single
    // already-merged file: nothing layered to conflict.
    let (config, conflicts): (ConfigLayer, Vec<String>) = match config_path {
        Some(path) => (load_yaml(path, "config")?, Vec::new()),
        None => (
            ConfigLayer::merge_layers(
                project::load_named_layers(&cwd)?
                    .into_iter()
                    .map(|(_, layer)| layer),
            ),
            super::verdict::layer_conflicts(&cwd)?,
        ),
    };
    let verdict = super::verdict::verdict(
        &ctx,
        &workflow,
        &workflow_path,
        &config,
        conflicts,
        super::verdict::Reach::Workflow,
    )
    .await;

    // Verification-effectiveness findings, surfaced here too — right when
    // someone is already looking at this workflow — not only via `stats
    // --workflow`. Best effort: a project with no state root yet (nothing
    // ever ran) or an unnamed workflow simply shows nothing, the same
    // stance `list_workflows` takes on missing history.
    let opened = super::stats::history(&ctx, &workflow.name).await;
    let (history, _) = super::stats::raw_history(&opened);
    let findings = yunta_engine::analyze_verification_effectiveness(&workflow, &history);
    let text = super::stats::render_verification_findings(&findings);
    if !text.is_empty() {
        note(format!("\n{text}"));
    }

    Ok(verdict.report(&ctx, workflow_path.display()).await)
}

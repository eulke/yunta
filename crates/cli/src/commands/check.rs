//! `yunta check <workflow>`: static validation of a workflow — no run
//! created, no agent involved. Resolves a bare catalog name the same way
//! `run` and `graph` do, merges the project's real config layers (or a
//! single explicit `--config` file), and reports every error the engine's
//! `check` finds, its warnings, and any verification-effectiveness
//! findings from this workflow's past runs.

use std::path::Path;

use yunta_core::text::problems;
use yunta_core::ConfigLayer;

use crate::context::Context;
use crate::error::{note, warn, CliError, Outcome};
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
    let config: ConfigLayer = match config_path {
        Some(path) => load_yaml(path, "config")?,
        None => {
            let layers = project::load_named_layers(&cwd)?;
            let named: Vec<(&str, &ConfigLayer)> =
                layers.iter().map(|(name, layer)| (*name, layer)).collect();
            let conflicts = yunta_core::permission_layer_conflicts(&named);
            if !conflicts.is_empty() {
                note(problems(workflow_path.display(), &conflicts));
                return Ok(Outcome::Reported);
            }
            ConfigLayer::merge_layers(layers.into_iter().map(|(_, layer)| layer))
        }
    };

    let mut errors = yunta_engine::check(&workflow, &config, &super::declared_capabilities);
    // Composition references (`use:`) resolve against the repo catalog
    // under `cwd` (`.yunta/workflows/`), then packs — the same catalog a
    // run's children resolve against at birth.
    let origin = yunta_engine::origin_of(&cwd, &workflow_path);
    errors.extend(yunta_engine::check_workflow_refs(
        &workflow, &config, &cwd, &origin,
    ));
    for warning in &yunta_engine::check_warnings(&workflow, &config) {
        warn(warning);
    }

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

    if errors.is_empty() {
        println!("{}: OK", workflow_path.display());
        Ok(Outcome::Success)
    } else {
        note(problems(workflow_path.display(), &errors));
        Ok(Outcome::Reported)
    }
}

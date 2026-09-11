//! `yunta resolve-gate <run_id> <option>`: answers a paused run's gate
//! decision from a separate process — no live surface attached to the
//! run itself, exactly the shape `yunta mcp`'s own `resolve_gate` tool
//! needs. Appends **only the decision** to the log
//! (`yunta_engine::resolve_gate` — decision and consequence are a
//! separate pair, and this only ever writes the former) and hands the
//! run off to a detached `yunta resume`, which consumes the pre-seeded
//! decision through the engine's one existing consequence path —
//! retry, abort, promote and internal gates alike. Same detach
//! mechanism as `run --detach`: a control-plane operation never blocks
//! for the run's own duration.

use yunta_core::events::HumanChoice;
use yunta_core::{Manifest, OptionId, Responder, RunId};

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::load_yaml;

pub async fn resolve_gate(
    run_id: &RunId,
    option: &OptionId,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let Some(run_dir) = ctx.project.run_dir(run_id.as_str()) else {
        return Err(CliError::msg(format!(
            "no run `{run_id}` under {} (or the default state root)",
            ctx.project.runs_root.display()
        )));
    };
    let manifest: Manifest = load_yaml(&run_dir.join("manifest.yaml"), "run manifest")?;

    let storage = ctx.async_storage().await?;

    yunta_engine::resolve_gate(
        &manifest,
        &storage,
        run_id,
        &ctx.clock,
        HumanChoice {
            option: option.clone(),
            by: crate::identity::responder(resolved_by),
            free_text: free_text.map(str::to_string),
        },
    )
    .await
    .map_err(|e| refused(run_id, e))?;

    super::spawn_detached_resume(&run_dir, run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| {
            CliError::msg(format!(
                "decision recorded, but {}",
                super::DetachedResumeError::new(run_id, source)
            ))
        })?;
    println!("run {run_id}: resolved `{option}`, driving forward independently");
    Ok(Outcome::Success)
}

/// A refusal the engine raised, in this command's own vocabulary.
///
/// One of them describes the state the run is in rather than anything
/// about the request, and a reader told the run is not parked wants to
/// see where it actually is. Which command shows that is the CLI's word,
/// not the engine's, so it is said here. Every other refusal already
/// names what to change — an option that is not on the menu lists the
/// ones that are — and passes through untouched.
fn refused(run_id: &RunId, error: yunta_engine::ResolveGateError) -> CliError {
    match error {
        yunta_engine::ResolveGateError::NotPaused => CliError::msg(format!(
            "{error} — `{}` shows where it is",
            super::advice::status(run_id)
        )),
        other => other.into(),
    }
}

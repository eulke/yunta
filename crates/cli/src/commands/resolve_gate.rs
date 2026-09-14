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
use yunta_core::{OptionId, Responder, RunId};

use crate::context::Context;
use crate::error::{CliError, Outcome};

pub async fn resolve_gate(
    run_id: &RunId,
    option: &OptionId,
    resolved_by: Option<&Responder>,
    free_text: Option<&str>,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
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
    .map_err(|refusal| CliError::gate_refused(run_id, refusal))?;

    super::spawn_detached_resume(&run_dir, run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| CliError::GateRecordedNotResumed {
            source: super::DetachedResumeError::new(run_id, source),
        })?;
    println!("run {run_id}: resolved `{option}`, driving forward independently");
    Ok(Outcome::Success)
}

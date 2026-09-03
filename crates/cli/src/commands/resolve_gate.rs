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

use yunta_core::{Manifest, RunId, SystemClock};
use yunta_storage::AsyncStorage;

use crate::error::{CliError, Outcome};
use crate::load_yaml;
use crate::project;

pub async fn resolve_gate(
    run_id: &RunId,
    option_id: &str,
    resolved_by: Option<&str>,
    free_text: Option<&str>,
) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let project = project::resolve(&cwd)?;
    let Some(run_dir) = project::find_run_dir(&project, run_id.as_str()) else {
        return Err(CliError::msg(format!(
            "no run `{run_id}` under {} (or the default state root)",
            project.runs_root.display()
        )));
    };
    let manifest: Manifest = load_yaml(&run_dir.join("manifest.yaml"), "run manifest")?;

    let storage = AsyncStorage::open(&project.storage_path).await?;

    yunta_engine::resolve_gate(
        &manifest,
        &storage,
        run_id,
        &SystemClock,
        option_id,
        resolved_by.map(str::to_string),
        free_text.map(str::to_string),
    )
    .await?;

    super::spawn_detached_resume(&run_dir, run_id.as_str(), &cwd).map_err(|e| {
        CliError::msg(format!(
            "decision recorded, but cannot spawn a detached `yunta resume {run_id}`: {e}"
        ))
    })?;
    println!("run {run_id}: resolved `{option_id}`, driving forward independently");
    Ok(Outcome::Success)
}

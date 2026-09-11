//! `yunta run --detach`: create the run, hand it to a fully independent
//! `yunta resume`, and report its id without waiting — the shape the MCP
//! control plane's `run_workflow` reaches a run in too, so neither ever
//! blocks for a run's duration.

use std::path::Path;

use yunta_core::{AdapterId, Manifest, ModeName, RunId};
use yunta_storage::AsyncStorage;

use super::{create_run_from, estimate, runnable};
use crate::commands::drive::RunJson;
use crate::commands::{spawn_detached_resume, DetachedResumeError};
use crate::context::Context;
use crate::error::{CliError, Outcome};

/// What a detached start needs to reach a run of its own: the same
/// workflow reference, inputs and overrides every run resolves, plus how
/// this invocation reports what it made.
pub(super) struct Detaching<'a> {
    pub(super) ctx: &'a Context,
    pub(super) storage: &'a AsyncStorage,
    pub(super) workflow_path: &'a Path,
    pub(super) raw_inputs: &'a [String],
    pub(super) adapter: Option<&'a AdapterId>,
    pub(super) mode: Option<&'a ModeName>,
    /// `--quiet`: the run id and nothing else on stdout. The budget
    /// warning §8.6 of the run contract keeps actionable still goes out.
    pub(super) quiet: bool,
    /// `--json`: one versioned document on stdout and nothing else.
    pub(super) json: bool,
}

/// Creates the run, hands it to a detached `yunta resume`, and reports
/// its id without waiting on it.
///
/// The estimation happens here, between freezing the manifest and
/// creating the run, because this invocation is the only one that can
/// carry it: §8.6 of the run contract gives the distribution — and the
/// budget warning derived from it — to whoever *creates* a run, and a
/// `yunta resume`, detached or not, picks one up instead. A warning that
/// asks whether a cap is worth starting under is worth nothing once the
/// child is already spending, so it goes out before the child exists.
pub(super) async fn detached(detaching: Detaching<'_>) -> Result<Outcome, CliError> {
    let Detaching {
        ctx,
        storage,
        workflow_path,
        raw_inputs,
        adapter,
        mode,
        quiet,
        json,
    } = detaching;
    // No fixture ever reaches here: a detached child resolves the
    // adapters `runners:` names and reads none.
    let (manifest, _) = runnable(ctx, workflow_path, raw_inputs, adapter, None).await?;
    estimate(ctx, storage, &manifest, quiet, json).await;
    let run_id = create_and_detach(ctx, storage, &manifest, mode).await?;
    if json {
        return crate::json::print_json(&RunJson::detached(&run_id));
    }
    println!("run {run_id}: detached, driving forward independently");
    Ok(Outcome::Success)
}

/// Creates a run for a real (non-mock) workflow and hands it to a detached
/// `yunta resume`, returning its id without waiting — the control plane's
/// `run_workflow`, resolving, checking, probing, freezing and creating
/// through the same [`runnable`] and [`create_and_detach`] pair
/// `yunta run --detach` uses, so both reach a run identically.
///
/// Says nothing about what this workflow has cost before: an agent client
/// reads the run id this returns, and §8.6 of the run contract hands that
/// client the same distribution through `list_workflows` — the surface it
/// consults while it is still choosing a workflow.
pub(crate) async fn start_detached(
    ctx: &Context,
    storage: &AsyncStorage,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mode: Option<&ModeName>,
) -> Result<RunId, CliError> {
    let (manifest, _) = runnable(ctx, workflow_path, raw_inputs, adapter, None).await?;
    create_and_detach(ctx, storage, &manifest, mode).await
}

/// Creates the run and hands it to a detached `yunta resume`, returning
/// its id without waiting on the child. Prints nothing: the caller
/// decides how to report the id (a line, or a DTO).
async fn create_and_detach(
    ctx: &Context,
    storage: &AsyncStorage,
    manifest: &Manifest,
    mode: Option<&ModeName>,
) -> Result<RunId, CliError> {
    let prepared = create_run_from(ctx, storage, manifest, mode).await?;
    spawn_detached_resume(&prepared.run_dir, prepared.run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| DetachedResumeError::new(&prepared.run_id, source))?;
    Ok(prepared.run_id)
}

//! `yunta run --detach`: create the run, hand it to a fully independent
//! `yunta resume`, and report its id without waiting — the shape the MCP
//! control plane's `run_workflow` reaches a run in too, so neither ever
//! blocks for a run's duration.

use std::path::Path;

use yunta_core::{AdapterId, Isolation, ModeName, Pid, RunId};
use yunta_engine::process::Supervision;
use yunta_engine::FrozenRun;
use yunta_storage::AsyncStorage;

use super::{create_run_from, preflight, runnable};
use crate::commands::drive::report_run_json;
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
    /// warning, which asks for a decision, still goes out.
    pub(super) quiet: bool,
    /// `--json`: one versioned document on stdout and nothing else.
    pub(super) json: bool,
}

/// Creates the run, hands it to a detached `yunta resume`, and reports
/// its id without waiting on it.
///
/// The estimation happens here, between freezing the manifest and
/// creating the run, because this invocation is the only one that can
/// carry it: the distribution — and the budget warning derived from it —
/// is said to whoever *creates* a run, and a
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
    let (frozen, _) = runnable(ctx, workflow_path, raw_inputs, adapter, None).await?;
    let preflight = preflight(ctx, &frozen.manifest, mode, quiet, json).await?;
    let run_id = create_and_detach(ctx, storage, &frozen, mode).await?;
    if json {
        // The run's own log, read the instant it was handed off: the
        // same document `yunta status --json` prints, saying where the
        // run stood when this invocation let go of it. The verdict is
        // the handoff's and not the run's — a run still moving is what
        // this command set out to leave behind.
        report_run_json(
            ctx,
            storage,
            &run_id,
            &frozen.manifest,
            preflight.warnings.clone(),
        )
        .await?;
        return Ok(Outcome::Success);
    }
    println!(
        "run {}: detached, driving forward independently",
        run_id.handle()
    );
    Ok(Outcome::Success)
}

/// Creates a run for a real (non-mock) workflow and hands it to a detached
/// `yunta resume`, returning its id without waiting — the control plane's
/// `run_workflow`, resolving, checking, probing, freezing and creating
/// through the same [`runnable`] and [`create_and_detach`] pair
/// `yunta run --detach` uses, so both reach a run identically.
///
/// Says nothing about what this workflow has cost before: an agent client
/// reads the run id this returns, and that client gets the same
/// distribution through `list_workflows` — the surface it
/// consults while it is still choosing a workflow.
pub(crate) async fn start_detached(
    ctx: &Context,
    storage: &AsyncStorage,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mode: Option<&ModeName>,
) -> Result<Started, CliError> {
    let (frozen, _) = runnable(ctx, workflow_path, raw_inputs, adapter, None).await?;
    // Quiet, because stdout here *is* the control plane's JSON-RPC
    // stream: the distribution line the estimation prints for a person
    // would land in the middle of a response. What it has to say
    // travels in the answer instead.
    let preflight = preflight(ctx, &frozen.manifest, mode, true, false).await?;
    let run_id = create_and_detach(ctx, storage, &frozen, mode).await?;
    Ok(Started {
        run_id,
        warnings: preflight.warnings,
    })
}

/// A run that was created and handed off, and what the caller owes its
/// reader about it.
pub(crate) struct Started {
    pub(crate) run_id: RunId,
    /// What was said before the first token: the budget warning, when this
    /// workflow's history has one to give, and the `files:` a node reads
    /// that the run would not find. A client that starts runs is the one
    /// deciding whether the run is worth starting, and it never sees
    /// stderr.
    pub(crate) warnings: crate::json::PreRunWarnings,
}

/// Creates the run and hands it to a detached `yunta resume`, returning
/// its id without waiting on the child. Prints nothing: the caller
/// decides how to report the id (a line, or a DTO).
async fn create_and_detach(
    ctx: &Context,
    storage: &AsyncStorage,
    frozen: &FrozenRun,
    mode: Option<&ModeName>,
) -> Result<RunId, CliError> {
    let prepared = create_run_from(ctx, storage, frozen, mode).await?;
    let isolation = frozen.manifest.isolation;
    // `create_run_from` claimed the checkout for *this* process, which is
    // about to leave. Whichever way the hand-off goes, the claim stops
    // describing who is in the tree: the child takes it over, or nobody
    // is in there and it is dropped. Left as it is, it names a dead
    // process the moment this one exits, and the next run takes over a
    // checkout the child is still working in.
    match spawn_detached_resume(&prepared.run_dir, prepared.run_id.as_str(), &ctx.cwd).await {
        Ok(child) => {
            hand_off(child, isolation, &ctx.cwd, ctx.teardown()).await?;
            Ok(prepared.run_id)
        }
        Err(source) => {
            yunta_engine::release_worktree(&ctx.cwd, isolation, ctx.teardown()).await?;
            Err(DetachedResumeError::new(&prepared.run_id, source).into())
        }
    }
}

/// Hands the claim to the child just spawned. When the hand-over fails, the
/// child is ended before the claim is released, so nothing keeps working in
/// the tree while the claim names nobody.
async fn hand_off(
    child: Pid,
    isolation: Isolation,
    cwd: &Path,
    teardown: Supervision<'_>,
) -> Result<(), CliError> {
    let Err(error) = yunta_engine::hand_over_worktree(cwd, isolation, child, teardown).await else {
        return Ok(());
    };
    if let Err(kill) = yunta_core::process::group::force_kill_group(child).await {
        tracing::warn!(%kill, "the detached child outlived a failed hand-over");
    }
    if let Err(release) = yunta_engine::release_worktree(cwd, isolation, teardown).await {
        tracing::warn!(%release, "the claim could not be released after a failed hand-over");
    }
    Err(error.into())
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::CommandExt;

    use tokio_util::sync::CancellationToken;
    use yunta_testkit_core::FixedClock;

    use super::*;

    #[tokio::test]
    async fn a_failed_hand_off_ends_the_child_before_the_claim_is_released() {
        // No common git directory here, so the hand-over cannot write its lock.
        let not_a_repo = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = Pid::try_from(child.id()).unwrap();
        let cancel = CancellationToken::new();
        let refused = hand_off(
            pid,
            Isolation::None,
            not_a_repo.path(),
            Supervision::outside_any_run(&cancel, &FixedClock),
        )
        .await;
        assert!(refused.is_err());
        yunta_testkit::wait_until_async(
            || std::future::ready(matches!(child.try_wait(), Ok(Some(_)))),
            || "the child outlived the failed hand-over".into(),
        )
        .await;
    }
}

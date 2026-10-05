//! What a run does when an invocation wakes one that already has
//! history: it verifies that the run is still what its own log says it
//! is, then records that it woke.

use yunta_core::events::{EventPayload, FindingSeverity, RunEvent, RunResumedPayload};
use yunta_core::{Location, RelativePath};

use crate::artifacts::ArtifactIntegrity;
use crate::replay::RunView;
use crate::worktree::{RunWorktree, WorktreeIntegrity};

use super::{schedule, steps, RunCtx, RunError};

/// Wakes a run that already has history: verifies that the run is still
/// what its own log says it is, records the resume with the policy each
/// orphan node resolves to, and states what the verification could not
/// check.
///
/// The order is the whole of it. A run that cannot answer for its history
/// is broken with a diagnostic before anything says it resumed, never a
/// run that keeps going and hands a node something the log never saw.
/// What the verification could not do is said after, because by then the
/// run has woken.
pub(super) async fn resume(ctx: &RunCtx<'_>, view: &RunView) -> Result<(), RunError> {
    let artifacts = verify_before_waking(ctx, view).await?;
    record_resume(ctx, view).await?;
    if let Some(detail) = artifacts.unverifiable_detail() {
        ctx.engine_finding(
            None,
            "engine-artifact-store",
            FindingSeverity::Minor,
            "the run holds artifacts this binary cannot verify".to_string(),
            Location::run(
                RelativePath::of([crate::artifacts::store::OBJECTS_DIR]),
                None,
            ),
            detail,
        )
        .await?;
    }
    Ok(())
}

/// Asks the run's two questions about itself before it wakes, and returns
/// what the artifact half found so the caller can report what it could
/// not check.
///
/// The two ask different questions of different things, and the
/// difference is deliberate. An artifact is immutable, so it is verified
/// by content: every object the log names is read back and hashed against
/// its own name. A worktree is the work and changes by design, so only
/// its identity and its ancestry are verified — see [`WorktreeIntegrity`]
/// for why its content is not, and what the engine does instead with a
/// tree that moved.
///
/// A missing worktree is the one failure here that is not `broken`: the
/// run's own evidence is intact, so the error carries the remedy instead.
async fn verify_before_waking(
    ctx: &RunCtx<'_>,
    view: &RunView,
) -> Result<ArtifactIntegrity, RunError> {
    let artifacts = ArtifactIntegrity::of(ctx.run_dir, &view.events).await;
    if let Some(diagnostic) = artifacts.diagnostic(ctx.run_id) {
        return Err(steps::broken(ctx, diagnostic).await);
    }
    let worktree = WorktreeIntegrity::of(
        RunWorktree {
            run_id: ctx.run_id,
            path: ctx.worktree,
            base_commit: &ctx.manifest.base_commit,
            isolation: ctx.manifest.isolation,
        },
        ctx.root_supervision(),
    )
    .await?;
    if let Some(diagnostic) = worktree.diagnostic() {
        return Err(steps::broken(ctx, diagnostic).await);
    }
    Ok(artifacts)
}

/// Writes the `run_resumed` that says the run woke, carrying the policy
/// each orphan node resolves to. Whether they share one is the
/// payload's own arithmetic.
async fn record_resume(ctx: &RunCtx<'_>, view: &RunView) -> Result<(), RunError> {
    // A node the mode excludes never ran, so the orphans are the same
    // whichever nodes are in the mode.
    let policies = schedule::resume_policies(
        ctx.manifest.workflow.nodes.iter(),
        &view.state,
        ctx.manifest.config.resolved_on_interrupt(),
    );
    let woken =
        RunResumedPayload::new(policies, crate::process::execution_environment(ctx.ambient));
    let woken = match moved(ctx, view).await {
        Some(checkout) => woken.in_checkout(checkout),
        None => woken,
    };
    ctx.emit(None, EventPayload::Run(RunEvent::Resumed(woken)))
        .await?;
    Ok(())
}

/// The checkout the run wakes in, when it is not the one its log last
/// named: the one it worked in was given back while it was parked.
async fn moved(ctx: &RunCtx<'_>, view: &RunView) -> Option<std::path::PathBuf> {
    let named = view.state.run.checkout()?;
    let canonical = |path: &std::path::Path| {
        let path = path.to_path_buf();
        async move { tokio::fs::canonicalize(&path).await.unwrap_or(path) }
    };
    let here = canonical(ctx.worktree).await;
    (canonical(named).await != here).then_some(here)
}

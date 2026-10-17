//! `yunta resume <run_id>`: reload the frozen manifest from run.dir and
//! drive the run forward with the same `execute_run` that started it —
//! the log decides what remains.
//!
//! run.dir is looked up in search order (current config's runs root,
//! then the default) — and once the manifest is open, the worktree
//! comes from its *frozen* paths, so a `paths.*` change between `run`
//! and `resume` never loses the run.
//!
//! What a person watching sees is what `yunta run` shows, down to the
//! flags: two commands that execute the same thing have no business
//! reporting it differently.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use yunta_adapters::MOCK_SESSION_PREFIX;
use yunta_core::events::{EventPayload, PauseReason, SessionEvent, StoredEvent};
use yunta_core::{Clock, Manifest, RunId};
use yunta_engine::{PriorEstimation, RunState};

use super::registry::{adapters_for, servable, ScriptedPaths, ServableError};
use crate::commands::drive::{drive, Driving, Prepared};
use crate::commands::stats;
use crate::commands::status::progress;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::output::{progress, Mode};
use crate::render::state::RunWord;
use crate::render::Look;

/// A run this invocation is about to pick up, as its own frozen manifest
/// describes it: where its state lives and the tree it works in.
struct Parked {
    run_dir: PathBuf,
    manifest: Manifest,
    worktree: PathBuf,
    prior: Option<PriorEstimation>,
}

/// Opens the run `run_id` names, refusing one this project cannot reach
/// or this binary cannot run, and waits out the limit reset it is paused
/// on before anything is taken.
///
/// Everything comes from the manifest the run froze, never the project's
/// current config: a run never re-reads config after it is created, and
/// the worktree it works in is the one its own frozen roots name. The
/// refusals come before the reset wait, so a run that cannot resume here
/// is refused at once rather than after a wait.
///
/// `None` when a person interrupted that wait: the run stays parked and
/// nothing was taken. `output` decides whether the wait is said on the
/// terminal first.
async fn parked(
    ctx: &Context,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Option<Parked>, CliError> {
    let open = ctx.open_run(run_id).await?;
    let state = yunta_engine::derive(&open.events);
    let bound = crate::project::bound_checkout(&state);
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
    // Resolved before any checkout is taken, so a refused resume takes no
    // checkout and writes nothing; the wake resolves the same redactor again.
    yunta_engine::run_redactor(&manifest, &yunta_core::ProcessSecrets)?;
    if predates_frozen_adapter(&manifest, &open.events) {
        return Err(ServableError::MockBeforeFrozenAdapter.into());
    }
    servable(&manifest, state.run.mode()).await?;
    if !waited_out_reset(ctx, &state, run_id, output, look).await {
        return Ok(None);
    }
    let worktree = ctx
        .project
        .run_tree(&manifest, bound.as_deref(), run_id, &ctx.cwd);
    let worktree = still_or_again(ctx, &manifest, run_id, worktree).await?;
    let frame = progress::frame_of(run_id, &manifest, &state, ctx.clock.now());
    let prior = stats::budget_prior(ctx, &manifest, &frame, &open.events).await;
    Ok(Some(Parked {
        run_dir,
        manifest,
        worktree,
        prior,
    }))
}

/// A manifest with no adapter whose log holds a session the mock opened
/// predates frozen adapters: its fixture was never kept.
fn predates_frozen_adapter(manifest: &Manifest, events: &[StoredEvent]) -> bool {
    manifest.adapter.is_none()
        && events.iter().any(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(opened)))
                    if opened.session_id.as_str().starts_with(MOCK_SESSION_PREFIX)
            )
        })
}

/// The checkout the run works in on this wake: the one its log names
/// while it is still the run's, or else the one the project's pool finds
/// for the run's branch — given back while the run was parked, or taken
/// away. A run working in a person's checkout works where it always did.
async fn still_or_again(
    ctx: &Context,
    manifest: &Manifest,
    run_id: &RunId,
    named: PathBuf,
) -> Result<PathBuf, CliError> {
    if manifest.isolation != yunta_core::Isolation::Worktree {
        return Ok(named);
    }
    let run_dir = ctx.project.runs_root.join(run_id.as_str());
    let pool = ctx
        .project
        .pool_of(manifest, &ctx.cwd, &run_dir, std::sync::Arc::new(ctx.clock));
    Ok(pool
        .wake_checkout(run_id, &named, ctx.supervision())
        .await?)
}

/// Waits until `at`, the reset the run is paused on, before anything is
/// taken from it. An interrupt ends the wait first and returns `false`: the
/// run stays parked, and nothing was taken. A past instant returns at once.
async fn waited_until(at: DateTime<Utc>, interrupt: &tokio_util::sync::CancellationToken) -> bool {
    waited_out(|| crate::context::time_until(at), interrupt).await
}

/// Sleeps until `left` reads nothing left, asking it again after each sleep:
/// a timer runs on the monotonic clock, which drifts from the wall clock
/// `left` reads over a long wait, so one sleep can end before the instant.
async fn waited_out(
    left: impl Fn() -> std::time::Duration,
    interrupt: &tokio_util::sync::CancellationToken,
) -> bool {
    loop {
        let remaining = left();
        if remaining.is_zero() {
            return true;
        }
        tokio::select! {
            () = yunta_engine::process::wait_for_deadline(Some(
                tokio::time::Instant::now() + remaining,
            )) => {}
            () = interrupt.cancelled() => return false,
        }
    }
}

/// Waits, when the run is paused on a limit's reset, until that reset has
/// come. The wait is said once on the terminal first, when `output` is
/// `Human` and the reset lies ahead. Returns `false` when a person
/// interrupted it.
async fn waited_out_reset(
    ctx: &Context,
    state: &RunState,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> bool {
    let (Some(at), Some((reason, _))) = (super::reset_at(state), state.run.paused()) else {
        return true;
    };
    if !crate::context::time_until(at).is_zero() {
        progress(output, look, waiting_line(run_id, reason));
    }
    waited_until(at, ctx.cancellation()).await
}

/// The line a person reads when a resume waits for a limit's reset: the
/// run's handle, the word it is called by, and the sentence the log
/// recorded for the pause, which names the node and the instant.
fn waiting_line(run_id: &RunId, reason: &PauseReason) -> String {
    format!(
        "run {}: {} — {}",
        run_id.handle(),
        RunWord::Waiting,
        yunta_render::pause::sentence(reason)
    )
}

pub async fn resume(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let Some(Parked {
        run_dir,
        manifest,
        worktree,
        prior,
    }) = parked(&ctx, run_id, output, look).await?
    else {
        return Ok(crate::render::RunExit::Interrupted.outcome());
    };
    let adapters = adapters_for(
        &manifest,
        &ScriptedPaths {
            run_dir: &run_dir,
            worktree: &worktree,
        },
    )?;
    let storage = ctx.async_storage().await?;
    super::started(run_id, "resuming", output, look);
    let prepared = Prepared {
        run_id: run_id.clone(),
        run_dir,
        worktree,
    };
    drive(Driving {
        ctx: &ctx,
        storage: &storage,
        manifest: &manifest,
        prepared: &prepared,
        adapters,
        // A run parked on its token budget is read against the sample its
        // creation read, as `status` reads it, so the closing says the same
        // caution.
        prior,
        // The pre-run estimation is said to whoever creates a run; this
        // picks one up.
        warnings: Default::default(),
        output,
        look,
    })
    .await
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use tokio_util::sync::CancellationToken;
    use yunta_core::NodeId;

    use super::*;

    #[tokio::test]
    async fn an_interrupt_ends_the_wait_for_a_reset_still_ahead() {
        let interrupt = CancellationToken::new();
        interrupt.cancel();
        // A reset no test run reaches: the wait can only end by the interrupt.
        let ahead = Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap();
        assert!(!waited_until(ahead, &interrupt).await);
    }

    #[tokio::test]
    async fn a_wait_whose_sleep_ends_before_the_wall_clock_sleeps_again() {
        // The wall clock reads a moment left after the first sleep, as it does
        // when the timer ran ahead of it, and nothing left after the next.
        let asked = std::cell::Cell::new(0);
        let left = || {
            asked.set(asked.get() + 1);
            match asked.get() {
                1 | 2 => std::time::Duration::from_millis(1),
                _ => std::time::Duration::ZERO,
            }
        };
        assert!(waited_out(left, &CancellationToken::new()).await);
        assert_eq!(
            asked.get(),
            3,
            "the wait asks the wall clock until it has come"
        );
    }

    #[tokio::test]
    async fn a_reset_already_past_is_not_waited_for() {
        let past = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        assert!(waited_until(past, &CancellationToken::new()).await);
    }

    #[test]
    fn the_wait_line_names_the_run_its_word_and_the_node_the_reset_is_for() {
        let run_id = RunId::from_static("run-1");
        let at = chrono::Utc
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .unwrap();
        let reason = PauseReason::AwaitingReset {
            node: NodeId::from_static("build"),
            at,
        };
        let line = waiting_line(&run_id, &reason);
        assert!(
            line.starts_with(&format!(
                "run {}: waiting — node `build` waits for its limit to reset at ",
                run_id.handle()
            )),
            "{line}"
        );
    }
}

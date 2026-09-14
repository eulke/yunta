//! Driving a run to its stop and reporting how it stopped.
//!
//! One path, taken by both commands that execute a run: `yunta run` after
//! it creates one, `yunta resume` after it opens one off the log. What
//! differs between them is over by the time this starts — the run exists,
//! its manifest is frozen, and its tree is ready — so a stop reported
//! here reads the same whichever command reached it.

use std::path::{Path, PathBuf};

use yunta_core::{AdapterId, Clock, Manifest, RunId};
use yunta_engine::{PriorEstimation, RunEnv, RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::state::RunWord;
use crate::render::Glyphs;
use crate::surface::{
    Closing, ClosingEnv, Curtain, Delivery, Diagnostics, Outline, Surface, SurfaceEnv, TerminalEnv,
};

/// A run's identity and the paths it lives at — what the executing path,
/// the detaching path and a resume all name it by.
pub(crate) struct Prepared {
    pub(crate) run_id: RunId,
    pub(crate) run_dir: PathBuf,
    pub(crate) worktree: PathBuf,
}

/// What the invocation holds once the run exists and can be driven —
/// the same shape whether the run was just created or picked back up,
/// which is what makes `run` and `resume` report identically rather than
/// merely try to.
///
/// One field is asymmetric on purpose. The distribution of this
/// workflow's past runs belongs to the invocation that chose to start
/// one, so `prior` is what `run` derived before spending anything and
/// nothing at all on a `resume`, which is picking up a run whose cost is
/// already partly spent.
pub(crate) struct Driving<'a> {
    pub(crate) ctx: &'a Context,
    pub(crate) storage: &'a AsyncStorage,
    pub(crate) manifest: &'a Manifest,
    pub(crate) prepared: &'a Prepared,
    pub(crate) adapters: super::Adapters,
    /// `--adapter <id>`: every runner resolves to its candidate on this
    /// adapter, and the log records each candidate passed over.
    pub(crate) adapter_override: Option<AdapterId>,
    pub(crate) prior: Option<PriorEstimation>,
    /// The warning the pre-run estimation raised, carried so the
    /// document this invocation prints says it too — stderr reaches the
    /// person watching, and a `--json` reader is watching nothing.
    pub(crate) budget_warning: Option<String>,
    /// `--quiet`: the run id and nothing else, with the verdict in the
    /// exit code.
    pub(crate) quiet: bool,
    /// `--json`: one versioned document and no live surface at all.
    pub(crate) json: bool,
}

/// How this invocation shows the run — derived here rather than handed
/// in, so no caller can present the same stop a different way.
struct Presentation {
    delivery: Delivery,
    glyphs: Glyphs,
}

impl Presentation {
    /// What `--quiet` and the environment this process was handed allow.
    fn of(quiet: bool) -> Self {
        Self {
            delivery: Delivery::choose(quiet, &TerminalEnv::from_process()),
            glyphs: Glyphs::from_env(),
        }
    }
}

/// Everything this invocation holds while the run is being drawn: the
/// surface the progress goes on, the observer the engine feeds it
/// through, the console the run puts its questions to, and the token a
/// person stops it with.
///
/// One object because they are one arrangement over one terminal — the
/// prompt takes its turn with the surface through the curtain, the
/// interrupt says so above the region through the surface's own door,
/// and a run with no surface has none of it.
struct Watching {
    surface: Option<Surface>,
    observer: Option<std::sync::Arc<dyn yunta_engine::RunObserver>>,
    asking: crate::human_interaction::ConsoleInteraction,
    /// Ctrl-C, bridged to the run's root cancellation.
    cancel: tokio_util::sync::CancellationToken,
}

/// Executes the run under a live surface, chases any promotion to its
/// end, and closes the invocation out — the one path `yunta run` and
/// `yunta resume` both take once the run they drive is open.
pub(crate) async fn drive(env: Driving<'_>) -> Result<Outcome, CliError> {
    let shown = Presentation::of(env.quiet);
    let forge = super::real_forge(&env.manifest.config);
    let ambient = crate::project::process_env();
    let watching = watch(&env, &shown).await?;
    let root_cancel = watching.cancel.clone();
    let report = match yunta_engine::execute_run(RunEnv {
        run_id: &env.prepared.run_id,
        manifest: env.manifest,
        run_dir: &env.prepared.run_dir,
        worktree: &env.prepared.worktree,
        adapters: &env.adapters,
        storage: env.storage,
        clock: std::sync::Arc::new(env.ctx.clock),
        ids: &env.ctx.ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &watching.asking,
        forge: forge.as_deref(),
        cancel: Some(&root_cancel),
        adapter_override: env.adapter_override.as_ref(),
        ambient: Some(&ambient),
        secrets: Some(std::sync::Arc::new(yunta_core::ProcessSecrets)),
        observer: watching.observer.clone(),
        fence_hook: Some(env.ctx.fence_hook.clone()),
    })
    .await
    {
        Ok(report) => report,
        Err(e) => {
            close(watching.surface).await;
            return Err(e.into());
        }
    };
    finish(
        &env,
        &shown,
        forge.as_deref(),
        &root_cancel,
        watching,
        report,
    )
    .await
}

/// What this invocation draws on, asks on, and is stopped through.
///
/// `--json` gets no surface at all: the document is the whole story, so
/// the engine carries no observer, and a prompt has no region to take a
/// turn with.
///
/// The order is the arrangement's own: the cancellation bridge is armed
/// once there is a surface for it to say so through, because the line
/// it raises belongs above the region rather than around it.
async fn watch(env: &Driving<'_>, shown: &Presentation) -> Result<Watching, CliError> {
    let surface = if env.json {
        None
    } else {
        Some(
            Surface::open(SurfaceEnv {
                run_id: &env.prepared.run_id,
                manifest: env.manifest,
                prior: env.prior.as_ref(),
                storage: env.storage,
                clock: std::sync::Arc::new(env.ctx.clock),
                delivery: shown.delivery,
                glyphs: shown.glyphs,
            })
            .await?,
        )
    };
    let cancel = super::cancel_on_ctrl_c(
        surface
            .as_ref()
            .map_or_else(Diagnostics::none, Surface::diagnostics),
    );
    Ok(Watching {
        observer: surface.as_ref().and_then(Surface::observer),
        asking: crate::human_interaction::ConsoleInteraction::new(
            surface
                .as_ref()
                .map_or_else(Curtain::none, Surface::curtain),
            cancel.clone(),
        ),
        cancel,
        surface,
    })
}

/// Chases a promotion to its end, takes the surface down, and reports
/// the run that actually closed.
///
/// The chase happens before anything downstream looks at the report:
/// `execute_run` closes exactly one run, and creating the successor needs
/// the original checkout it is never given — see `promote.rs` for why. A
/// successor is the same invocation carrying on, so it draws on the same
/// surface and asks on the same console.
async fn finish(
    env: &Driving<'_>,
    shown: &Presentation,
    forge: Option<&dyn yunta_core::port::Forge>,
    cancel: &tokio_util::sync::CancellationToken,
    watching: Watching,
    report: RunReport,
) -> Result<Outcome, CliError> {
    let (run_id, manifest, worktree, report) = super::promote::drive_promotions(
        &super::promote::PromotionEnv {
            cwd: &env.ctx.cwd,
            project: &env.ctx.project,
            storage: env.storage,
            ids: &env.ctx.ids,
            adapters: &env.adapters,
            forge,
            cancel: Some(cancel),
            human_interaction: &watching.asking,
            observer: watching.observer.clone(),
        },
        env.prepared.run_id.clone(),
        env.manifest.clone(),
        env.prepared.worktree.clone(),
        report,
    )
    .await
    .map_err(CliError::msg)?;
    close(watching.surface).await;

    settle(Settling {
        ctx: env.ctx,
        storage: env.storage,
        run_id,
        manifest,
        worktree,
        report,
        cancelled: cancel.is_cancelled(),
        prior: env.prior.as_ref(),
        budget_warning: env.budget_warning.clone(),
        glyphs: shown.glyphs,
        quiet: env.quiet,
        json: env.json,
    })
    .await
}

/// Takes the live surface down, drawing everything the engine handed it
/// first, so nothing lands on a terminal that still has a region pinned
/// to it.
pub(crate) async fn close(surface: Option<Surface>) {
    if let Some(surface) = surface {
        surface.close().await;
    }
}

/// How an invocation that drove a run to its stop was asked to report
/// it, and everything the report is made of.
pub(crate) struct Settling<'a> {
    pub(crate) ctx: &'a Context,
    pub(crate) storage: &'a AsyncStorage,
    /// The last run of the chain: a promotion successor when the run
    /// promoted, the run itself otherwise.
    pub(crate) run_id: RunId,
    pub(crate) manifest: Manifest,
    pub(crate) worktree: PathBuf,
    pub(crate) report: RunReport,
    /// Whether a person interrupted this invocation.
    pub(crate) cancelled: bool,
    pub(crate) prior: Option<&'a PriorEstimation>,
    /// The pre-run warning, for the document this invocation prints.
    /// `None` on a `resume`: §8.6 gives the estimation to whoever
    /// *creates* a run, and a resume picks one up.
    pub(crate) budget_warning: Option<String>,
    pub(crate) glyphs: Glyphs,
    pub(crate) quiet: bool,
    pub(crate) json: bool,
}

/// Releases what the run held and reports how it ended — the tail every
/// command that executes a run shares, so `run` and `resume` cannot
/// report the same stop differently.
pub(crate) async fn settle(settling: Settling<'_>) -> Result<Outcome, CliError> {
    // Only a *finished* run releases isolation `none`'s lock — a paused
    // run expects a future `resume` on the same checkout, which is the
    // same logical run, not a second concurrent one. A user cancellation
    // also releases it: the engine process is exiting, and a Ctrl-C is
    // designed to leave nothing held.
    if matches!(
        settling.report.terminal,
        RunTerminal::Finished | RunTerminal::Failed { .. }
    ) || settling.cancelled
    {
        yunta_engine::release_worktree(
            &settling.ctx.cwd,
            settling.manifest.isolation,
            yunta_engine::process::Supervision::none(),
        )
        .await?;
    }
    if settling.json {
        return Ok(report_run_json(
            settling.ctx,
            settling.storage,
            &settling.run_id,
            &settling.manifest,
            settling.budget_warning,
        )
        .await?
        .into());
    }
    if settling.quiet {
        // The run id already went out when the invocation opened, and the
        // verdict travels in the exit code.
        return Ok(verdict(&settling.report));
    }
    report_closing(Closed {
        run_id: &settling.run_id,
        manifest: &settling.manifest,
        run_dir: &run_dir_of(settling.ctx, &settling.run_id),
        worktree: &settling.worktree,
        storage: settling.storage,
        clock: &settling.ctx.clock,
        prior: settling.prior,
        glyphs: settling.glyphs,
    })
    .await
}

/// Where the run this invocation drove lives, by the same search order
/// `yunta status` follows: a run may sit under the default state root
/// rather than under this project's. A run whose directory nothing
/// found is named under this project's root, because the block that
/// closes it out prints a path a reader goes looking in, and no path at
/// all sends them nowhere.
fn run_dir_of(ctx: &Context, run_id: &RunId) -> PathBuf {
    ctx.project
        .run_dir(run_id.as_str())
        .unwrap_or_else(|| ctx.project.runs_root.join(run_id.as_str()))
}

/// Everything the closing block reads about a run that has stopped, as
/// the invocation that drove it holds it.
pub(crate) struct Closed<'a> {
    pub(crate) run_id: &'a RunId,
    /// The run's own frozen manifest — what says which workflow it ran,
    /// which branch it started from, and how it isolated its tree.
    pub(crate) manifest: &'a Manifest,
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
    pub(crate) storage: &'a AsyncStorage,
    pub(crate) clock: &'a dyn Clock,
    /// What this workflow's past runs cost, for the block's own
    /// comparison; `None` below the history floor.
    pub(crate) prior: Option<&'a PriorEstimation>,
    pub(crate) glyphs: Glyphs,
}

/// Reads the run's log one last time and prints the block that closes it
/// out, handing back the verdict the exit code carries.
///
/// The log, not the report the engine handed back: a run's state is
/// derived from its own events, so the last word on what happened comes
/// off the same source every other surface reads.
pub(crate) async fn report_closing(closed: Closed<'_>) -> Result<Outcome, CliError> {
    let events = closed.storage.events_for_run(closed.run_id.clone()).await?;
    let closing = Closing::of(ClosingEnv {
        run_id: closed.run_id,
        workflow: &closed.manifest.workflow,
        events: &events,
        prior: closed.prior,
        now: closed.clock.now(),
        decision: yunta_engine::current_escalation(closed.manifest, &yunta_engine::derive(&events))
            .map(|(node, escalation)| (node, escalation.into_payload())),
        outline: Outline {
            run_dir: closed.run_dir,
            worktree: closed.worktree,
            base_branch: &closed.manifest.base_branch,
            isolation: closed.manifest.isolation,
        },
    });
    print!("{}", closing.render(closed.glyphs));
    Ok(closing.outcome())
}

/// The verdict the exit code carries: success only when the run
/// finished. A paused, failed or unresolved-promoted run ran to a stop
/// that needs a decision, which is its own output, not an error.
pub(crate) fn verdict(report: &RunReport) -> Outcome {
    RunWord::of_terminal(&report.terminal).into()
}

/// Prints the run as the one versioned document `run --json`,
/// `resume --json` and `run --detach --json` all emit, and hands back
/// the word it reports.
///
/// The document is derived from the run's own log, exactly as
/// `yunta status --json` derives it later: the invocation that drove a
/// run and the command that reads it an hour afterwards publish one
/// answer, rather than two shapes that happen to agree.
///
/// The word, not a verdict: whether the invocation succeeded is the
/// caller's to say. An invocation that drove the run answers for where
/// the run got to; one that handed it off answers for the handoff, and
/// a run still moving is exactly what that command set out to leave
/// behind.
pub(crate) async fn report_run_json(
    ctx: &Context,
    storage: &AsyncStorage,
    run_id: &RunId,
    manifest: &Manifest,
    budget_warning: Option<String>,
) -> Result<RunWord, CliError> {
    let events = storage.events_for_run(run_id.clone()).await?;
    let document = crate::json::RunDocument::of(run_id, &events, manifest, ctx.clock.now())
        .warning(budget_warning);
    crate::json::print_json(&document)?;
    Ok(document.outcome())
}

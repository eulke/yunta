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
use crate::json::SCHEMA_VERSION;
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
        observer: watching.observer.clone(),
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
    forge: Option<&dyn yunta_adapters::Forge>,
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
        yunta_engine::release_worktree(&settling.ctx.cwd, settling.manifest.isolation).await?;
    }
    if settling.json {
        return print_run_json(&settling.run_id, &settling.report, settling.budget_warning);
    }
    if settling.quiet {
        // The run id already went out when the invocation opened, and the
        // verdict travels in the exit code.
        return Ok(verdict(&settling.report));
    }
    report_closing(Closed {
        run_id: &settling.run_id,
        manifest: &settling.manifest,
        // Search order, the same rule `status` follows: the run may live
        // under the default state root rather than this project's.
        run_dir: &settling
            .ctx
            .project
            .run_dir(settling.run_id.as_str())
            .unwrap_or_else(|| {
                settling
                    .ctx
                    .project
                    .runs_root
                    .join(settling.run_id.as_str())
            }),
        worktree: &settling.worktree,
        storage: settling.storage,
        clock: &settling.ctx.clock,
        prior: settling.prior,
        glyphs: settling.glyphs,
    })
    .await
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
        decision: yunta_engine::current_escalation(closed.manifest, &events),
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

/// Prints the run's outcome as the one versioned document `run --json`
/// and `resume --json` both emit, and reports its verdict.
pub(crate) fn print_run_json(
    run_id: &RunId,
    report: &RunReport,
    budget_warning: Option<String>,
) -> Result<Outcome, CliError> {
    crate::json::print_json(&RunJson::from_report(run_id, report, budget_warning))?;
    Ok(verdict(report))
}

/// The verdict the exit code carries: success only when the run
/// finished. A paused, failed or unresolved-promoted run ran to a stop
/// that needs a decision, which is its own output, not an error.
pub(crate) fn verdict(report: &RunReport) -> Outcome {
    match report.terminal {
        RunTerminal::Finished => Outcome::Success,
        _ => Outcome::Reported,
    }
}

/// The outcome of a run as the versioned JSON `--json` prints and
/// the control plane can hand back — the run's id and how it ended, or
/// that it detached to run on independently.
#[derive(serde::Serialize)]
pub(crate) struct RunJson {
    schema_version: u32,
    run_id: String,
    /// The one piece of the pre-run estimation §8.6 of the run contract
    /// makes actionable: the declared cap sits under what this workflow
    /// has historically spent. It goes to stderr for the person
    /// watching, and here for the reader that has only this document —
    /// which is the reader most likely to be automating the spend.
    #[serde(skip_serializing_if = "Option::is_none")]
    budget_warning: Option<String>,
    /// `detached`, or the run's terminal: `finished`, `paused`,
    /// `failed`, `promoted`.
    outcome: &'static str,
    /// Why, when the run did not finish. Absent on a clean finish and on
    /// `detached`, where there is nothing to say yet. A caller reading
    /// JSON learns what went wrong here, not only that something did.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl RunJson {
    pub(crate) fn detached(run_id: &RunId, budget_warning: Option<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            budget_warning,
            outcome: "detached",
            reason: None,
        }
    }

    fn from_report(run_id: &RunId, report: &RunReport, budget_warning: Option<String>) -> Self {
        let (outcome, reason) = match &report.terminal {
            RunTerminal::Finished => ("finished", None),
            RunTerminal::Paused { reason } => ("paused", Some(reason.clone())),
            RunTerminal::Failed { reason } => ("failed", Some(reason.clone())),
            RunTerminal::Promoted { .. } => ("promoted", None),
        };
        Self {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            budget_warning,
            outcome,
            reason,
        }
    }
}

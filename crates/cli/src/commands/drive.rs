//! Driving a run to its stop and reporting how it stopped.
//!
//! One path, taken by both commands that execute a run: `yunta run` after
//! it creates one, `yunta resume` after it opens one off the log. What
//! differs between them is over by the time this starts — the run exists,
//! its manifest is frozen, and its tree is ready — so a stop reported
//! here reads the same whichever command reached it.

mod watch;

use watch::{watch, Watching};

use std::path::{Path, PathBuf};

use yunta_core::{AdapterId, Clock, Manifest, RunId};
use yunta_engine::{PriorEstimation, RunEnv, RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::Glyphs;
use crate::surface::{Closing, ClosingEnv, Delivery, Outline, TerminalEnv};

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

/// Executes the run under a live surface, chases any promotion to its
/// end, and closes the invocation out — the one path `yunta run` and
/// `yunta resume` both take once the run they drive is open.
pub(crate) async fn drive(env: Driving<'_>) -> Result<Outcome, CliError> {
    let shown = Presentation::of(env.quiet);
    let forge = super::real_forge(&env.manifest.config);
    let watching = watch(&env, &shown).await?;
    let root_cancel = watching.cancel.clone();
    let report = match execute(Executing {
        run_id: &env.prepared.run_id,
        manifest: env.manifest,
        run_dir: &env.prepared.run_dir,
        worktree: &env.prepared.worktree,
        adapters: &env.adapters,
        storage: env.storage,
        clock: std::sync::Arc::new(env.ctx.clock),
        ids: &env.ctx.ids,
        human_interaction: &watching.asking,
        forge: forge.as_deref(),
        cancel: &root_cancel,
        adapter_override: env.adapter_override.as_ref(),
        observer: watching.observer.clone(),
        fence_hook: env.ctx.fence_hook.clone(),
        ambient: &env.ctx.env,
    })
    .await
    {
        Ok(report) => report,
        Err(e) => {
            watching.close().await;
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

/// One run, handed to the engine.
///
/// Everything below this line is the same for the run an invocation was
/// given and for every successor a promotion makes: the retry ceiling,
/// the ambient environment, the secrets source, how a `RunEnv` is
/// filled in. It used to be written twice — here and in the promotion
/// loop — and the two agreed only by attention, so a field added to one
/// was a field missing from the other.
pub(crate) struct Executing<'a> {
    pub(crate) run_id: &'a RunId,
    pub(crate) manifest: &'a Manifest,
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
    pub(crate) adapters: &'a super::Adapters,
    pub(crate) storage: &'a AsyncStorage,
    pub(crate) clock: std::sync::Arc<dyn Clock>,
    pub(crate) ids: &'a dyn yunta_core::IdSource,
    pub(crate) human_interaction: &'a dyn yunta_engine::HumanInteraction,
    pub(crate) forge: Option<&'a dyn yunta_core::port::Forge>,
    pub(crate) cancel: &'a tokio_util::sync::CancellationToken,
    /// `--adapter <id>`: every runner resolves to its candidate on this
    /// adapter. A successor never carries one — the override belongs to
    /// the invocation that asked for it, and a promotion is the run
    /// carrying on, not a new invocation.
    pub(crate) adapter_override: Option<&'a AdapterId>,
    pub(crate) observer: Option<std::sync::Arc<dyn yunta_engine::RunObserver>>,
    pub(crate) fence_hook: yunta_core::fence::FenceHook,
    pub(crate) ambient: &'a yunta_core::Env,
}

/// Runs one run to its stop — the one `execute_run` call this binary
/// makes.
pub(crate) async fn execute(on: Executing<'_>) -> Result<RunReport, yunta_engine::RunError> {
    yunta_engine::execute_run(RunEnv {
        run_id: on.run_id,
        manifest: on.manifest,
        run_dir: on.run_dir,
        worktree: on.worktree,
        adapters: on.adapters,
        storage: on.storage,
        clock: on.clock,
        ids: on.ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: on.human_interaction,
        forge: on.forge,
        cancel: on.cancel,
        adapter_override: on.adapter_override,
        ambient: Some(on.ambient),
        secrets: Some(std::sync::Arc::new(yunta_core::ProcessSecrets)),
        observer: on.observer,
        fence_hook: Some(on.fence_hook),
    })
    .await
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
            ctx: env.ctx,
            storage: env.storage,
            adapters: &env.adapters,
            forge,
            human_interaction: &watching.asking,
            observer: watching.observer.clone(),
        },
        env.prepared.run_id.clone(),
        env.manifest.clone(),
        env.prepared.worktree.clone(),
        report,
    )
    .await?;
    watching.close().await;

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
    released(&settling).await?;
    if settling.json {
        return Ok(report_run_json(
            settling.ctx,
            settling.storage,
            &settling.run_id,
            &settling.manifest,
            settling.budget_warning,
        )
        .await?
        .verdict());
    }
    if settling.quiet {
        // The run id already went out when the invocation opened, and
        // the verdict travels in the exit code — read off the run's own
        // log, the same reading the document and the closing block take.
        return Ok(documented(
            settling.ctx,
            settling.storage,
            &settling.run_id,
            &settling.manifest,
            settling.budget_warning,
        )
        .await?
        .verdict());
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

/// Hands back what the run held, when the run is done holding it.
///
/// Only a *finished* run releases isolation `none`'s lock — a paused
/// run expects a future `resume` on the same checkout, which is the
/// same logical run, not a second concurrent one. A user cancellation
/// also releases it: the engine process is exiting, and a Ctrl-C is
/// designed to leave nothing held.
async fn released(settling: &Settling<'_>) -> Result<(), CliError> {
    let done = matches!(
        settling.report.terminal,
        RunTerminal::Finished | RunTerminal::Failed { .. }
    ) || settling.cancelled;
    if done {
        yunta_engine::release_worktree(
            &settling.ctx.cwd,
            settling.manifest.isolation,
            settling.ctx.teardown(),
        )
        .await?;
    }
    Ok(())
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

/// The run as its own log describes it, carrying the pre-run warning
/// this invocation holds — the one reading every way of reporting a run
/// takes, printed or not.
///
/// It is derived from the log, exactly as `yunta status --json` derives
/// it later: the invocation that drove a run and the command that reads
/// that run an hour afterwards publish one answer, rather than two
/// shapes that happen to agree.
async fn documented(
    ctx: &Context,
    storage: &AsyncStorage,
    run_id: &RunId,
    manifest: &Manifest,
    budget_warning: Option<String>,
) -> Result<crate::json::RunDocument, CliError> {
    let events = storage.events_for_run(run_id.clone()).await?;
    Ok(
        crate::json::RunDocument::of(run_id, &events, manifest, ctx.clock.now())
            .warning(budget_warning),
    )
}

/// Prints the run as the one versioned document `run --json`,
/// `resume --json` and `run --detach --json` all emit, and hands the
/// document back.
///
/// The document, not a verdict: whether the invocation succeeded is the
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
) -> Result<crate::json::RunDocument, CliError> {
    let document = documented(ctx, storage, run_id, manifest, budget_warning).await?;
    crate::json::print_json(&document)?;
    Ok(document)
}

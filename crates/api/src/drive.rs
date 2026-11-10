//! See [`crate`]. Driving a run here: the run this process holds, executed
//! with the ports a surface lends it, each promotion it closes with followed
//! onto its successor as the same drive carrying on, and what it held handed
//! back. Every interface that drives a run where it was asked drives it
//! through here, so a run behaves the same whichever drives it.
//!
//! Once the run stops: a run that finished, failed or was stopped gives back
//! the checkout a run that isolates nothing locked, while a paused run keeps
//! it for its resume; and a run paused on a limit's reset is handed to a
//! detached resume that waits for the reset, since nothing else would resume
//! it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{MockAdapter, MockFixture, RunPaths};
use yunta_core::events::{self, DecisionOn, GateWaitingPayload, PauseReason};
use yunta_core::port::{Forge, HumanInteraction, RunObserver};
use yunta_core::rail::RailCommand;
use yunta_core::shown::ShownDocument;
use yunta_core::{Env, Manifest, ModeName, NodeId, Pid, RunAdapter, RunId};
use yunta_engine::{RunEnv, RunHold, RunReport, RunTerminal};
use yunta_render::document::{PreRunWarnings, RunDocument};
use yunta_replay::history::PriorEstimation;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;
use yunta_storage::AsyncStorage;

use crate::machine::Adapters;
use crate::{Error, Failure, HeldRun, Project, Remarks, Stop};

/// What a surface lends a run it drives here: who the run asks when it needs
/// a person, and who watches what it does as it does it.
pub struct Ports<'a> {
    /// A console a person answers at, or nobody.
    pub person: &'a dyn HumanInteraction,
    /// What draws each event and each word as the run makes it.
    pub observer: Option<Arc<dyn RunObserver>>,
}

/// A run driven here to its stop: the last run of its chain — a promotion's
/// successor when it promoted — with the manifest it froze, the tree it
/// worked in, and its state when it stopped.
pub struct Driven<'p> {
    project: &'p Project,
    pub id: RunId,
    /// The run's directory, where the closing block says it lives.
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub worktree: PathBuf,
    pub state: RunState,
    /// How it stopped.
    pub ending: Ending,
    /// What this workflow's past runs cost, for the closing's comparison.
    pub prior: Option<PriorEstimation>,
    /// Every caution said before the first token, for the document.
    pub warnings: PreRunWarnings,
}

/// How a run driven here stopped. A run that closes promoted is followed onto
/// its successor, so the run a drive hands back never ends promoted.
#[derive(Debug, Clone, PartialEq)]
pub enum Ending {
    Finished,
    /// The pause it stopped on, as its log records it.
    Paused {
        reason: PauseReason,
    },
    /// A node failed and the run's `defaults.on_failure` closed the run as
    /// failed rather than pausing it.
    Failed {
        node: NodeId,
        failure: events::Failure,
    },
}

/// What a run that stopped reads like to the block that closes it: its state
/// off its log, the decision it stopped on with what that decision shows, and
/// the instant it was read at.
pub struct ClosingReading {
    pub state: RunState,
    pub decision: Option<(DecisionOn, GateWaitingPayload)>,
    pub shown: Vec<ShownDocument>,
    pub now: DateTime<Utc>,
}

/// One run of a chain, handed to the engine with the claim this process
/// holds on it.
pub(crate) struct Execution<'a> {
    pub(crate) id: &'a RunId,
    pub(crate) manifest: &'a Manifest,
    pub(crate) hold: RunHold,
    pub(crate) worktree: &'a Path,
}

/// What every run of a chain is driven with: the sessions' adapters, the
/// forge, the log, the ports the surface lent, and the environment its
/// nodes read: the workspace's, or none for a run as hermetic as a case.
pub(crate) struct Means<'a> {
    pub(crate) adapters: Adapters,
    pub(crate) forge: Option<Arc<dyn Forge>>,
    pub(crate) storage: AsyncStorage,
    pub(crate) ports: Ports<'a>,
    pub(crate) ambient: &'a Env,
}

impl<'p> HeldRun<'p> {
    /// Drives the run here to its stop with `ports`, following each promotion
    /// onto its successor, and hands back what it held. A forge the config
    /// declares that cannot be reached is said through `remarks`, and its
    /// gates fall back to the person.
    pub async fn drive(
        self,
        ports: Ports<'_>,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Driven<'p>, Error> {
        let project = self.project;
        let adapters = adapters_for(&self.manifest, &self.dir, &self.worktree)?;
        let storage = project.storage().await?;
        let means = Means {
            adapters,
            forge: crate::forge::forge_for(
                &self.manifest.config,
                project.workspace.secrets.as_ref(),
                remarks,
            ),
            storage,
            ports,
            ambient: &project.workspace.env,
        };
        let execution = Execution {
            id: &self.id,
            manifest: &self.manifest,
            hold: self.hold,
            worktree: &self.worktree,
        };
        let report = project.execute(execution, &means, stop).await?;
        let worktree = report.tree(&self.worktree);
        let (id, manifest, worktree, report) = project
            .promotions((self.id, self.manifest, worktree, report), &means, stop)
            .await?;
        project.released(&manifest, &report, stop).await?;
        let dir = project.run_dir_of(&id);
        project
            .resumed_after_reset(&id, &dir, &report.state, stop)
            .await?;
        let ending = match report.terminal {
            RunTerminal::Finished | RunTerminal::Promoted { .. } => Ending::Finished,
            RunTerminal::Paused { reason } => Ending::Paused { reason },
            RunTerminal::Failed { node, failure } => Ending::Failed { node, failure },
        };
        Ok(Driven {
            project,
            id,
            dir,
            manifest,
            worktree,
            state: report.state,
            ending,
            prior: self.prior,
            warnings: self.warnings,
        })
    }
}

impl Driven<'_> {
    /// The run as the block that closes it reads it: off its own log, the
    /// same source every other surface reads, rather than the report the
    /// engine handed back.
    pub async fn closing(&self) -> Result<ClosingReading, Error> {
        self.project
            .closing(&self.id, &self.dir, &self.manifest)
            .await
    }

    /// The run's document as this drive leaves it, with the cautions said
    /// before its first token.
    pub async fn document(&self) -> Result<RunDocument, Error> {
        let warnings = self.warnings.clone();
        self.project
            .run_document(&self.id, &self.manifest, warnings)
            .await
    }
}

impl Project {
    /// Run `id`, whose directory is `dir` and which froze `manifest`, as the
    /// block that closes it reads it once it stopped: off its own log, the
    /// same source every other surface reads.
    pub async fn closing(
        &self,
        id: &RunId,
        dir: &Path,
        manifest: &Manifest,
    ) -> Result<ClosingReading, Error> {
        let storage = self.storage().await?;
        let events = storage
            .events_for_run(id.clone())
            .await
            .map_err(Failure::new)?;
        let state = RunState::replay(&events);
        let decision = yunta_engine::current_escalation(manifest, dir, &state)
            .await?
            .map(|(node, escalation)| (node, escalation.into_payload()));
        let shown = match &decision {
            Some((_, escalation)) => {
                let shows = escalation.shows();
                let asked = manifest.request();
                yunta_engine::shown_documents(dir, shows, &state, asked).await?
            }
            None => Vec::new(),
        };
        Ok(ClosingReading {
            state,
            decision,
            shown,
            now: self.workspace.clock.now(),
        })
    }

    /// Run `id`'s document as the process that drove it leaves it: derived
    /// from its log, exactly as `status --json` derives it later, so the
    /// invocation that drove a run and the one that reads it an hour later
    /// publish one answer. It carries `warnings`, the cautions said before
    /// the run's first token, which a reader with no terminal reads only here.
    pub async fn run_document(
        &self,
        id: &RunId,
        manifest: &Manifest,
        warnings: PreRunWarnings,
    ) -> Result<RunDocument, Error> {
        let storage = self.storage().await?;
        let events = storage
            .events_for_run(id.clone())
            .await
            .map_err(Failure::new)?;
        let state = RunState::replay(&events);
        // This process is the engine that drove the run.
        let engine = Engine::this(Pid::current());
        let decision = match self.run_dir(id.as_str()) {
            Some(dir) => yunta_engine::current_escalation(manifest, &dir, &state).await?,
            None => None,
        };
        let now = self.workspace.clock.now();
        let mut frame =
            yunta_replay::frame::RunFrame::of(id, &manifest.workflow, &state, None, &engine, now);
        frame.prior = self.budget_prior(manifest, &frame, &events).await;
        let document = RunDocument::of(id, &events, &state, manifest, &frame, decision);
        Ok(document.warnings(warnings))
    }

    /// Runs one run of a chain to its stop. The claim ends as this returns,
    /// before anything hands the run on: a detached resume that waits for a
    /// reset would otherwise find its own parent holding the run.
    pub(crate) async fn execute(
        &self,
        execution: Execution<'_>,
        means: &Means<'_>,
        stop: &Stop,
    ) -> Result<RunReport, Error> {
        let workspace = &self.workspace;
        let report = yunta_engine::execute_run(RunEnv {
            run_id: execution.id,
            manifest: execution.manifest,
            hold: &execution.hold,
            worktree: execution.worktree,
            adapters: &means.adapters,
            storage: &means.storage,
            clock: Arc::clone(&workspace.clock),
            ids: workspace.ids.as_ref(),
            human_interaction: means.ports.person,
            forge: means.forge.as_deref(),
            cancel: stop.work(),
            ambient: Some(means.ambient),
            secrets: Arc::clone(&workspace.secrets),
            observer: means.ports.observer.clone(),
            rail_command: Some(RailCommand::new(workspace.binary.clone())),
        })
        .await?;
        Ok(report)
    }

    /// Follows each promotion a run closes with onto the successor it asks
    /// for, driven as the same drive carrying on, and hands back the run that
    /// actually closed: `execute_run` closes exactly one run, and creating
    /// its successor needs the checkout it worked in.
    async fn promotions(
        &self,
        (mut id, mut manifest, mut worktree, mut report): (RunId, Manifest, PathBuf, RunReport),
        means: &Means<'_>,
        stop: &Stop,
    ) -> Result<(RunId, Manifest, PathBuf, RunReport), Error> {
        while let RunTerminal::Promoted { suggested_mode } = &report.terminal {
            let mode: ModeName = suggested_mode.clone();
            let successor = self
                .successor(&id, &manifest, &worktree, &mode, means, stop)
                .await?;
            let clock = self.workspace.clock.as_ref();
            let contention = yunta_engine::lock::Contention::Refuse;
            let hold = yunta_engine::hold_run(&successor.run_dir, contention, clock, stop.work())
                .await
                .map_err(yunta_engine::RunError::from)?;
            let execution = Execution {
                id: &successor.run_id,
                manifest: &successor.manifest,
                hold,
                worktree: &successor.worktree,
            };
            let successor_report = self.execute(execution, means, stop).await?;
            // Where the successor works as it stopped: a move may have taken
            // it out of the checkout it was handed.
            worktree = successor_report.tree(&successor.worktree);
            id = successor.run_id;
            manifest = successor.manifest;
            report = successor_report;
        }
        Ok((id, manifest, worktree, report))
    }

    /// The successor a promotion of run `id` to `mode` asks for, created by
    /// the engine as a `kind: workflow` child that promotes is.
    async fn successor(
        &self,
        id: &RunId,
        manifest: &Manifest,
        worktree: &Path,
        mode: &ModeName,
        means: &Means<'_>,
        stop: &Stop,
    ) -> Result<yunta_engine::PromotionSuccessor, Error> {
        let workspace = &self.workspace;
        let successor = yunta_engine::create_promotion_successor(
            yunta_engine::Predecessor {
                id,
                manifest,
                worktree,
                run_dir: &self.runs_root.join(id.as_str()),
            },
            mode,
            yunta_engine::RunRoots {
                runs: &self.runs_root,
                worktrees: &self.worktrees_root,
            },
            yunta_engine::CallerInfra {
                storage: &means.storage,
                ids: workspace.ids.as_ref(),
                supervision: workspace.supervision(stop),
                environment: yunta_engine::process::execution_environment(Some(&workspace.env)),
                secrets: workspace.secrets.as_ref(),
            },
        )
        .await?;
        Ok(successor)
    }

    /// Gives back what the run held when it is done holding it: only a run
    /// that finished or failed, or one the caller stopped, gives back the
    /// lock a run that isolates nothing holds on this checkout — a paused run
    /// waits for its resume on the same checkout.
    async fn released(
        &self,
        manifest: &Manifest,
        report: &RunReport,
        stop: &Stop,
    ) -> Result<(), Error> {
        let done = matches!(
            report.terminal,
            RunTerminal::Finished | RunTerminal::Failed { .. }
        ) || stop.is_stopped();
        if done {
            let teardown = self.workspace.teardown(stop);
            yunta_engine::release_worktree(&self.workspace.dir, manifest.isolation, teardown)
                .await?;
        }
        Ok(())
    }

    /// Hands a run paused on a limit's reset to a detached resume that waits
    /// for the reset before it touches anything, so the caller may leave: a
    /// run the caller stopped starts nothing more.
    async fn resumed_after_reset(
        &self,
        id: &RunId,
        dir: &Path,
        state: &RunState,
        stop: &Stop,
    ) -> Result<(), Error> {
        if crate::detach::reset_at(state).is_none() || stop.is_stopped() {
            return Ok(());
        }
        crate::detach::resume_detached(&self.workspace, id, dir).await?;
        Ok(())
    }

    /// Where run `id` lives, by the search every reader takes; a run nothing
    /// found is named under this project's runs root, since a closing block
    /// prints a path a reader goes looking in.
    fn run_dir_of(&self, id: &RunId) -> PathBuf {
        self.run_dir(id.as_str())
            .unwrap_or_else(|| self.runs_root.join(id.as_str()))
    }
}

/// The registry the manifest's sessions run on: the adapters `runners:`
/// names, or every session scripted by the mock fixture the run froze, its
/// claims kept in the run's directory so a resumed run goes on from the
/// scripts its earlier sessions claimed.
fn adapters_for(manifest: &Manifest, dir: &Path, worktree: &Path) -> Result<Adapters, Error> {
    adapters_and_script(manifest, dir, worktree).map(|(adapters, _)| adapters)
}

/// The registry the manifest's sessions run on, with the scripted mock a
/// mock run's registry is built on, so a caller can ask which of its scripts
/// no session claimed.
pub(crate) fn adapters_and_script(
    manifest: &Manifest,
    dir: &Path,
    worktree: &Path,
) -> Result<(Adapters, Option<Arc<MockAdapter>>), Error> {
    match &manifest.adapter {
        Some(RunAdapter::Mock { fixture }) => {
            let claims = dir.join(yunta_adapters::MOCK_CLAIMS_FILE);
            let mock = Arc::new(
                scripted(fixture, dir, worktree)?
                    .keeping_claims(claims)
                    .map_err(Failure::new)?,
            );
            let adapters = yunta_adapters::registry(&manifest.config, Arc::clone(&mock));
            Ok((adapters, Some(mock)))
        }
        _ => Ok((crate::machine::real_adapters(&manifest.config), None)),
    }
}

/// A fixture the run's sessions are scripted by that does not parse.
#[derive(Debug, thiserror::Error)]
#[error("the run's fixture does not parse: {0}")]
struct FixtureUnparsed(yunta_adapters::FixtureError);

/// The mock a run's sessions take their scripts from: its fixture rendered
/// against the run's own directories, and parsed.
fn scripted(fixture: &str, dir: &Path, worktree: &Path) -> Result<MockAdapter, Error> {
    let staging = yunta_engine::run_dir::staging_root(dir);
    let paths = RunPaths {
        run_dir: dir,
        worktree,
        staging: &staging,
    };
    MockFixture::parse(fixture, &paths)
        .map(MockAdapter::new)
        .map_err(|source| Failure::new(FixtureUnparsed(source)).into())
}

#[cfg(test)]
mod tests;

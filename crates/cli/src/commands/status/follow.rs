//! `yunta status <run> --follow`: a run followed from a process that did not
//! start it.
//!
//! The follower reads the run's log once, opens the surface `yunta run` draws
//! with what it read, and then reads only what came past the last seq it holds,
//! at the pace of [`Pace`], handing each event to the surface and to its own
//! state. Where the engine stands comes from the run's registry, read once a
//! second, the way `yunta status` reads it, and what the run's sessions say
//! from the socket that registry names, through `listen`. When the run stops
//! it takes the surface down and prints the block `yunta run` leaves, read
//! from the log the same way, and exits with the code that block gives.
//!
//! A run that closes promoted is followed onto its successor, as `yunta run`
//! carries on with one, along the API's `PromotionChain`. Whether the run still
//! moves is its frame's to say, so every interface that follows a run stops
//! where this one does.
//!
//! A follower only reads: it takes no lock, writes no event, touches no registry
//! and never signals the engine. A run that is not moving has nothing to follow,
//! so it gets the page `yunta status` prints and the code its stop gives. Ctrl-C
//! stops the follower alone: the run goes on in the process that drives it.

use std::path::PathBuf;
use std::time::Duration;

use surco::look::Look;
use surco::words::Standing;
use tokio_util::sync::CancellationToken;
use yunta_api::{EventLog, Project, PromotionChain, RunRegistry, Stop};
use yunta_core::events::StoredEvent;
use yunta_core::pace::Pace;
use yunta_core::{Manifest, RunId, Seq};
use yunta_render::page::LeftRunning;
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::history::PriorEstimation;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

use crate::commands::drive::{report_closing, Closed};
use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::{Delivery, EngineReading, Surface, SurfaceEnv, TerminalEnv};

mod listen;

/// How often the follower asks the log what came past the last seq it holds. A
/// tenth of a second puts an event on the follower's terminal well inside the
/// quarter second a person reads as at once, and a run that did not move costs
/// one indexed read that returns nothing.
const READ_EVERY: Duration = Duration::from_millis(100);

/// How long the follower waits for the successor of a run that closed
/// promoted: the engine that promotes creates it once it has closed the run
/// and set its tree up, which takes a few seconds on a large repository.
const SUCCESSOR_WAIT: Duration = Duration::from_secs(30);

/// How often the follower reads the run's registry. Once a second tells a gone
/// engine within the two seconds a stall is owed, with no read of the process
/// table for every read of the log.
const REGISTRY_EVERY: Duration = Duration::from_secs(1);

/// Follows `run_id`, and answers with the code its stop gives.
pub(crate) async fn follow(
    shell: &Shell,
    run_id: &RunId,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let stop = shell.stop();
    let open = project.run(run_id).await?;
    let now = project.workspace.clock.now();
    let state = RunState::replay(&open.events);
    let registry = open.registry();
    let engine = registry.engine();
    let manifest = open.manifest.doc;
    let frame = RunFrame::of(run_id, &manifest.workflow, &state, None, &engine, now);
    if ending(&frame).is_some() {
        let standing = super::print_page(&project, run_id, look).await?;
        return Ok(crate::output::exit(standing));
    }
    let started = open.events.first().map(|event| event.timestamp);
    let prior = project
        .prior_estimation(&manifest, &frame.mode, started)
        .await;
    let mut followed = Followed {
        run_id: run_id.clone(),
        run_dir: open.dir,
        registry,
        manifest,
        prior,
        events: open.events,
        state,
        engine,
        registry_read_at: now,
        listening: listen::Listening::default(),
    };
    let log = project.event_log().await?;
    let mut chain = PromotionChain::starting_at(&log, run_id.clone()).await?;
    let surface = followed.surface(&project, &log);
    let ended = followed
        .read_until_it_stops(&project, &stop, &log, &mut chain, &surface)
        .await?;
    surface.close().await;
    answer(ended, &followed, &project, run_id, look).await
}

/// What following `run_id` answers once it ended `ended`: the block that
/// closes out the run it followed last, the page of a run whose engine is
/// gone, or that the run goes on.
async fn answer(
    ended: Ended,
    followed: &Followed,
    project: &Project,
    run_id: &RunId,
    look: &Look,
) -> Result<Outcome, CliError> {
    match ended {
        Ended::Stopped => followed.closing(project, look).await,
        // What drove the run is gone, so no block closes it out: the page says
        // what holds it.
        Ended::Stalled => {
            let standing = super::print_page(project, run_id, look).await?;
            Ok(crate::output::exit(standing))
        }
        Ended::Interrupted => {
            crate::output::show(look, &LeftRunning { run_id });
            Ok(crate::output::Outcome::interrupted())
        }
    }
}

/// How following a run ended.
#[derive(Debug)]
enum Ended {
    /// The run is no longer moving.
    Stopped,
    /// Its log says it moves, and the engine its registry names is gone.
    Stalled,
    /// A person stopped the follower, and the run goes on.
    Interrupted,
}

/// Waits one look's interval, or until `stop` trips: whether it tripped.
async fn paced(pace: &Pace, stop: &CancellationToken) -> bool {
    tokio::select! {
        () = pace.beat() => false,
        () = stop.cancelled() => true,
    }
}

/// How following a run `frame` reads ends, or `None` while the run moves. An
/// absent registry is not a stall: a run handed to a detached process has
/// none for an instant.
fn ending(frame: &RunFrame) -> Option<Ended> {
    match (frame.moves(), frame.standing) {
        (true, _) => None,
        (false, Standing::Stalled) => Some(Ended::Stalled),
        (false, _) => Some(Ended::Stopped),
    }
}

/// The run being followed, as far as the follower has read it.
struct Followed {
    run_id: RunId,
    run_dir: PathBuf,
    /// Where the process driving the run is read from.
    registry: RunRegistry,
    manifest: Manifest,
    /// What this workflow's runs cost before this one was created: the history
    /// `yunta run` compared it with, so its region and its close compare alike.
    prior: Option<PriorEstimation>,
    /// Every event read, so a log that stops making sense is derived whole,
    /// which is what says where it broke.
    events: Vec<StoredEvent>,
    state: RunState,
    /// The process driving the run, as its registry said when last read.
    engine: Engine,
    /// When the registry was last read: the follower reads it again once a
    /// second has passed.
    registry_read_at: chrono::DateTime<chrono::Utc>,
    /// The socket of the run, heard while its registry names it.
    listening: listen::Listening,
}

impl Followed {
    /// The surface `yunta run` draws, seeded with what was read, and reading the
    /// process driving the run from its registry.
    fn surface(&self, project: &Project, log: &EventLog) -> Surface {
        Surface::open(
            SurfaceEnv {
                run_id: &self.run_id,
                manifest: &self.manifest,
                prior: self.prior.as_ref(),
                log,
                clock: std::sync::Arc::clone(&project.workspace.clock),
                delivery: Delivery::choose(false, TerminalEnv::settled()),
                glyphs: crate::output::glyphs(),
                engine: EngineReading::Registry(self.registry.clone()),
            },
            self.events.clone(),
        )
    }

    /// Reads the log past the last seq held, at the follower's pace, until the
    /// run is no longer moving or the invocation's stop trips. A run that closed
    /// promoted is followed onto its successor.
    async fn read_until_it_stops(
        &mut self,
        project: &Project,
        stop: &Stop,
        log: &EventLog,
        chain: &mut PromotionChain,
        surface: &Surface,
    ) -> Result<Ended, CliError> {
        let pace = Pace::every(READ_EVERY);
        loop {
            if paced(&pace, stop.work()).await {
                return Ok(Ended::Interrupted);
            }
            let past = log.events_after(&self.run_id, self.last()).await?;
            for event in past {
                self.take(&event);
                surface.deliver(event).await;
            }
            let frame = self.frame(project.workspace.clock.now());
            self.listening.to(self.engine.words(), surface.observer());
            match ending(&frame) {
                None => {}
                Some(Ended::Stopped) if matches!(frame.phase, RunPhase::Promoted { .. }) => {
                    let successor = self.onto_successor(project, stop, log, chain, surface);
                    if let Some(ended) = successor.await? {
                        return Ok(ended);
                    }
                }
                Some(ended) => return Ok(ended),
            }
        }
    }

    /// Waits for the successor of a run that closed promoted and goes on with
    /// it: `None` once it does, or how following ends when no successor comes.
    async fn onto_successor(
        &mut self,
        project: &Project,
        stop: &Stop,
        log: &EventLog,
        chain: &mut PromotionChain,
        surface: &Surface,
    ) -> Result<Option<Ended>, CliError> {
        let pace = Pace::every(READ_EVERY);
        let since = project.workspace.clock.now();
        let waited = || {
            (project.workspace.clock.now() - since)
                .to_std()
                .unwrap_or_default()
        };
        while waited() < SUCCESSOR_WAIT {
            if let Some(successor) = chain.successor(log).await? {
                self.go_on_with(project, successor, surface).await?;
                return Ok(None);
            }
            if paced(&pace, stop.work()).await {
                return Ok(Some(Ended::Interrupted));
            }
        }
        Ok(Some(Ended::Stopped))
    }

    /// Follows `successor` from here on: the surface is handed its log, which is
    /// what moves the region onto it, and the follower reads it as its own.
    async fn go_on_with(
        &mut self,
        project: &Project,
        successor: RunId,
        surface: &Surface,
    ) -> Result<(), CliError> {
        let open = project.run(&successor).await?;
        for event in &open.events {
            surface.deliver(event.clone()).await;
        }
        let now = project.workspace.clock.now();
        self.registry = open.registry();
        self.engine = self.registry.engine();
        self.run_id = successor;
        self.run_dir = open.dir;
        self.manifest = open.manifest.doc;
        self.state = RunState::replay(&open.events);
        self.events = open.events;
        self.registry_read_at = now;
        Ok(())
    }

    /// The seq of the last event read; a run is never followed before its
    /// first event is on the log.
    fn last(&self) -> Seq {
        self.events.last().map_or(Seq::FIRST, |event| event.seq)
    }

    /// Folds one event into the state, deriving the whole log again when it
    /// does not fold, which is what marks where the log broke.
    fn take(&mut self, event: &StoredEvent) {
        self.events.push(event.clone());
        if self.state.apply(event).is_err() {
            self.state = RunState::replay(&self.events);
        }
    }

    /// The run's frame at `now`, with the registry read again once it is a
    /// second old.
    fn frame(&mut self, now: chrono::DateTime<chrono::Utc>) -> RunFrame {
        let stale = (now - self.registry_read_at)
            .to_std()
            .is_ok_and(|age| age >= REGISTRY_EVERY);
        if stale {
            self.engine = self.registry.engine();
            self.registry_read_at = now;
        }
        RunFrame::of(
            &self.run_id,
            &self.manifest.workflow,
            &self.state,
            None,
            &self.engine,
            now,
        )
    }

    /// The block `yunta run` leaves when the run stops, read from the log, and
    /// the code it gives.
    async fn closing(&self, project: &Project, look: &Look) -> Result<Outcome, CliError> {
        let bound = self.state.run.checkout().map(std::path::Path::to_path_buf);
        let here = &project.workspace.dir;
        let worktree = project.run_tree(&self.manifest, bound.as_deref(), &self.run_id, here);
        let reading = project
            .closing(&self.run_id, &self.run_dir, &self.manifest)
            .await?;
        let closed = Closed {
            run_id: &self.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &worktree,
            prior: self.prior.as_ref(),
            cwd: &project.workspace.dir,
            home: project.workspace.env.home.as_deref(),
        };
        Ok(report_closing(&closed.closing(reading), look))
    }
}

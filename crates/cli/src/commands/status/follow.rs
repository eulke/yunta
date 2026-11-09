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
//! carries on with one. The predecessor's log does not name it: the successor's
//! `run_created` names the run it was promoted from, so the follower looks among
//! the runs whose heads it had not seen when it began.
//!
//! A follower only reads: it takes no lock, writes no event, touches no registry
//! and never signals the engine. A run that is not moving has nothing to follow,
//! so it gets the page `yunta status` prints and the code its stop gives. Ctrl-C
//! stops the follower alone: the run goes on in the process that drives it.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use surco::look::Look;
use surco::words::Standing;
use tokio_util::sync::CancellationToken;
use yunta_api::{EventLog, RunRegistry};
use yunta_core::events::{EventPayload, RunEvent, StoredEvent};
use yunta_core::pace::Pace;
use yunta_core::{Manifest, RunId, Seq};
use yunta_render::page::LeftRunning;
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::history::PriorEstimation;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

use crate::commands::drive::{report_closing, Closed};
use crate::context::Context;
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
    let ctx = Context::of(shell)?;
    let open = ctx.project.run(run_id).await?;
    let now = ctx.clock.now();
    let state = RunState::replay(&open.events);
    let registry = open.registry();
    let engine = registry.engine();
    let manifest = open.manifest.doc;
    let frame = RunFrame::of(run_id, &manifest.workflow, &state, None, &engine, now);
    if ending(frame.standing, &frame.phase).is_some() {
        let standing = super::print_page(&ctx, run_id, look).await?;
        return Ok(crate::output::exit(standing));
    }
    let started = open.events.first().map(|event| event.timestamp);
    let prior = ctx
        .project
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
        seen: HashSet::new(),
        listening: listen::Listening::default(),
    };
    let log = ctx.project.event_log().await?;
    followed.seen = log.runs().await?.into_iter().collect();
    let surface = followed.surface(&ctx, &log);
    let ended = followed.read_until_it_stops(&ctx, &log, &surface).await?;
    surface.close().await;
    answer(ended, &followed, (&ctx, &log), run_id, look).await
}

/// What following `run_id` answers once it ended `ended`: the block that
/// closes out the run it followed last, the page of a run whose engine is
/// gone, or that the run goes on.
async fn answer(
    ended: Ended,
    followed: &Followed,
    (ctx, log): (&Context, &EventLog),
    run_id: &RunId,
    look: &Look,
) -> Result<Outcome, CliError> {
    match ended {
        Ended::Stopped => followed.closing(ctx, log, look).await,
        // What drove the run is gone, so no block closes it out: the page says
        // what holds it.
        Ended::Stalled => {
            let standing = super::print_page(ctx, run_id, look).await?;
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

/// How following a run that stands at `standing` in `phase` ends, or `None`
/// while it is created or running with an engine that lives. An absent registry
/// is not a stall: a run handed to a detached process has none for an instant.
fn ending(standing: Standing, phase: &RunPhase) -> Option<Ended> {
    if !matches!(phase, RunPhase::Created | RunPhase::Running) {
        return Some(Ended::Stopped);
    }
    (standing == Standing::Stalled).then_some(Ended::Stalled)
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
    /// Every run whose head the follower has looked at, which no successor of
    /// this run can be.
    seen: HashSet<RunId>,
    /// The socket of the run, heard while its registry names it.
    listening: listen::Listening,
}

impl Followed {
    /// The surface `yunta run` draws, seeded with what was read, and reading the
    /// process driving the run from its registry.
    fn surface(&self, ctx: &Context, log: &EventLog) -> Surface {
        Surface::open(
            SurfaceEnv {
                run_id: &self.run_id,
                manifest: &self.manifest,
                prior: self.prior.as_ref(),
                log,
                clock: std::sync::Arc::clone(&ctx.clock),
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
        ctx: &Context,
        log: &EventLog,
        surface: &Surface,
    ) -> Result<Ended, CliError> {
        let pace = Pace::every(READ_EVERY);
        loop {
            if paced(&pace, ctx.cancellation()).await {
                return Ok(Ended::Interrupted);
            }
            let past = log.events_after(&self.run_id, self.last()).await?;
            for event in past {
                self.take(&event);
                surface.deliver(event).await;
            }
            let frame = self.frame(ctx.clock.now());
            self.listening.to(self.engine.words(), surface.observer());
            match ending(frame.standing, &frame.phase) {
                None => {}
                Some(Ended::Stopped) if matches!(frame.phase, RunPhase::Promoted { .. }) => {
                    if let Some(ended) = self.onto_successor(ctx, log, surface).await? {
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
        ctx: &Context,
        log: &EventLog,
        surface: &Surface,
    ) -> Result<Option<Ended>, CliError> {
        let pace = Pace::every(READ_EVERY);
        let since = ctx.clock.now();
        let waited = || (ctx.clock.now() - since).to_std().unwrap_or_default();
        while waited() < SUCCESSOR_WAIT {
            if let Some(successor) = self.successor(log).await? {
                self.go_on_with(ctx, successor, surface).await?;
                return Ok(None);
            }
            if paced(&pace, ctx.cancellation()).await {
                return Ok(Some(Ended::Interrupted));
            }
        }
        Ok(Some(Ended::Stopped))
    }

    /// The run promoted from this one, among the runs whose heads the follower
    /// had not seen, or `None` while there is none yet.
    async fn successor(&mut self, log: &EventLog) -> Result<Option<RunId>, CliError> {
        for run in log.runs().await? {
            if !self.seen.insert(run.clone()) {
                continue;
            }
            let events = log.events(&run).await?;
            let promoted_from = events.first().and_then(|first| match first.payload() {
                Some(EventPayload::Run(RunEvent::Created(created))) => {
                    created.promoted_from.clone()
                }
                _ => None,
            });
            if promoted_from.as_ref() == Some(&self.run_id) {
                return Ok(Some(run));
            }
        }
        Ok(None)
    }

    /// Follows `successor` from here on: the surface is handed its log, which is
    /// what moves the region onto it, and the follower reads it as its own.
    async fn go_on_with(
        &mut self,
        ctx: &Context,
        successor: RunId,
        surface: &Surface,
    ) -> Result<(), CliError> {
        let open = ctx.project.run(&successor).await?;
        for event in &open.events {
            surface.deliver(event.clone()).await;
        }
        let now = ctx.clock.now();
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
    async fn closing(
        &self,
        ctx: &Context,
        log: &EventLog,
        look: &Look,
    ) -> Result<Outcome, CliError> {
        let bound = self.state.run.checkout().map(std::path::Path::to_path_buf);
        let worktree =
            ctx.project
                .run_tree(&self.manifest, bound.as_deref(), &self.run_id, &ctx.cwd);
        report_closing(Closed {
            run_id: &self.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &worktree,
            log,
            clock: ctx.clock.as_ref(),
            prior: self.prior.as_ref(),
            look,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::NodeState;
    use yunta_core::NodeId;
    use yunta_replay::frame::{NodeStanding, RunPhase};

    use super::*;

    /// How a run reads with one node running, under a process whose registry
    /// says `engine`.
    fn under(engine: Engine) -> (Standing, RunPhase) {
        let nodes = vec![yunta_replay::frame::NodeFrame::sample(
            &NodeId::from_static("build"),
            NodeStanding::Reached(NodeState::Running { attempt: 1 }),
        )];
        let phase = RunPhase::Running;
        (
            yunta_replay::frame::run_standing(&phase, &nodes, 0, &engine),
            phase,
        )
    }

    /// A run handed to a detached process has no registry for an instant,
    /// and a follower that called that a stall would end on a run about to
    /// move. Only an engine its registry names, and that is gone, stalls it.
    #[test]
    fn an_absent_registry_is_not_a_stall() {
        let (standing, phase) = under(Engine::Unrecorded);
        assert!(ending(standing, &phase).is_none());
        let (standing, phase) = under(Engine::Dead);
        assert!(matches!(ending(standing, &phase), Some(Ended::Stalled)));
    }
}

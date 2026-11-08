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
use std::sync::Arc;
use std::time::Duration;

use surco::components::{Gist, Headline, Next};
use surco::doc::Doc;
use surco::look::Look;
use surco::words::Standing;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, RunEvent, StoredEvent};
use yunta_core::step::NextStep;
use yunta_core::{Clock, Manifest, RunId, Seq};
use yunta_engine::pace::Pace;
use yunta_render::command;
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::history::PriorEstimation;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;
use yunta_storage::AsyncStorage;

use crate::commands::drive::{report_closing, Closed};
use crate::context::Context;
use crate::error::{CliError, Outcome};
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
pub(crate) async fn follow(run_id: &RunId, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let now = ctx.clock.now();
    let state = RunState::replay(&open.events);
    let engine = yunta_engine::read_engine(&open.run_dir, &yunta_engine::lock::SystemProbe);
    let manifest = open.manifest.doc;
    let frame = super::progress::frame_of(run_id, &manifest, &state, &engine, now);
    if ending(frame.standing, &frame.phase).is_some() {
        let standing = super::print_page(&ctx, run_id, look).await?;
        return Ok(crate::render::exit(standing));
    }
    let started = open.events.first().map(|event| event.timestamp);
    let sample = crate::commands::stats::budget_sample(&ctx, &manifest, &frame.mode, started).await;
    let mut followed = Followed {
        run_id: run_id.clone(),
        run_dir: open.run_dir,
        manifest,
        prior: yunta_replay::history::PriorEstimation::of(&sample),
        events: open.events,
        state,
        engine,
        registry_read_at: now,
        seen: HashSet::new(),
        listening: listen::Listening::default(),
    };
    let storage = ctx.async_storage().await?;
    followed.seen = storage
        .heads()
        .await?
        .into_iter()
        .map(|head| head.run_id)
        .collect();
    let surface = followed.surface(&ctx, &storage);
    let ended = followed
        .read_until_it_stops(&ctx, &storage, &surface)
        .await?;
    surface.close().await;
    match ended {
        Ended::Stopped => followed.closing(&ctx, &storage, look).await,
        // What drove the run is gone, so no block closes it out: the page says
        // what holds it.
        Ended::Stalled => {
            let standing = super::print_page(&ctx, run_id, look).await?;
            Ok(crate::render::exit(standing))
        }
        Ended::Interrupted => {
            crate::output::document(look, stopped_following(run_id));
            Ok(crate::render::RunExit::Interrupted.outcome())
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

/// What a person who stopped following a run is told: it goes on without this
/// terminal, and the command that follows it again.
fn stopped_following(run_id: &RunId) -> Doc {
    let handle = run_id.handle();
    Doc::new()
        .with(Headline::new(
            Standing::Running.mark(),
            format!("run {handle}"),
            Standing::Running.word(),
        ))
        .with(Gist::new(
            "this terminal stopped following it; the run goes on without it",
        ))
        .with(Next {
            steps: vec![(
                command::line(&NextStep::Follow {
                    run: run_id.clone(),
                }),
                "follows it again",
            )],
        })
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
    fn surface(&self, ctx: &Context, storage: &AsyncStorage) -> Surface {
        Surface::open(
            SurfaceEnv {
                run_id: &self.run_id,
                manifest: &self.manifest,
                prior: self.prior.as_ref(),
                storage,
                clock: Arc::new(ctx.clock),
                delivery: Delivery::choose(false, TerminalEnv::settled()),
                glyphs: crate::render::glyphs(),
                engine: EngineReading::Registry(self.run_dir.clone()),
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
        storage: &AsyncStorage,
        surface: &Surface,
    ) -> Result<Ended, CliError> {
        let pace = Pace::every(READ_EVERY);
        loop {
            if paced(&pace, ctx.cancellation()).await {
                return Ok(Ended::Interrupted);
            }
            let past = storage
                .events_after(self.run_id.clone(), self.last())
                .await?;
            for event in past {
                self.take(&event);
                surface.deliver(event).await;
            }
            let frame = self.frame(ctx.clock.now());
            self.listening.to(self.engine.words(), surface.observer());
            match ending(frame.standing, &frame.phase) {
                None => {}
                Some(Ended::Stopped) if matches!(frame.phase, RunPhase::Promoted { .. }) => {
                    if let Some(ended) = self.onto_successor(ctx, storage, surface).await? {
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
        storage: &AsyncStorage,
        surface: &Surface,
    ) -> Result<Option<Ended>, CliError> {
        let pace = Pace::every(READ_EVERY);
        let since = ctx.clock.now();
        let waited = || (ctx.clock.now() - since).to_std().unwrap_or_default();
        while waited() < SUCCESSOR_WAIT {
            if let Some(successor) = self.successor(storage).await? {
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
    async fn successor(&mut self, storage: &AsyncStorage) -> Result<Option<RunId>, CliError> {
        for head in storage.heads().await? {
            if !self.seen.insert(head.run_id.clone()) {
                continue;
            }
            let events = storage.events_for_run(head.run_id.clone()).await?;
            let promoted_from = events.first().and_then(|first| match first.payload() {
                Some(EventPayload::Run(RunEvent::Created(created))) => {
                    created.promoted_from.clone()
                }
                _ => None,
            });
            if promoted_from.as_ref() == Some(&self.run_id) {
                return Ok(Some(head.run_id));
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
        let open = ctx.open_run(&successor).await?;
        for event in &open.events {
            surface.deliver(event.clone()).await;
        }
        let now = ctx.clock.now();
        self.run_id = successor;
        self.run_dir = open.run_dir;
        self.manifest = open.manifest.doc;
        self.state = RunState::replay(&open.events);
        self.events = open.events;
        self.engine = yunta_engine::read_engine(&self.run_dir, &yunta_engine::lock::SystemProbe);
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
            self.engine =
                yunta_engine::read_engine(&self.run_dir, &yunta_engine::lock::SystemProbe);
            self.registry_read_at = now;
        }
        super::progress::frame_of(&self.run_id, &self.manifest, &self.state, &self.engine, now)
    }

    /// The block `yunta run` leaves when the run stops, read from the log, and
    /// the code it gives.
    async fn closing(
        &self,
        ctx: &Context,
        storage: &AsyncStorage,
        look: &Look,
    ) -> Result<Outcome, CliError> {
        let bound = crate::project::bound_checkout(&self.state);
        let worktree =
            ctx.project
                .run_tree(&self.manifest, bound.as_deref(), &self.run_id, &ctx.cwd);
        report_closing(Closed {
            run_id: &self.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &worktree,
            storage,
            clock: &ctx.clock,
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

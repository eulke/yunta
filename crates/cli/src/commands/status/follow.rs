//! `yunta status <run> --follow`: a run followed from a process that did not
//! start it.
//!
//! The follower follows the run through the API's [`Following`], at one pace for every
//! interface that follows a run: the run read whole first, with every event of its log,
//! then what comes, the events past those read, what the run's sessions say, and the
//! successor a promotion hands the work to, read whole too. The follower keeps the run
//! as the following read it and folds each batch into it by the API's one fold. It
//! opens the surface `yunta run` draws with the events read whole and hands it each
//! event and what was said. It stops by the rule every interface stops a following by,
//! once the run's frame no longer moves and the following awaits no successor; then it
//! takes the surface down and prints the block `yunta run` leaves, read from the log the
//! same way, and exits with the code that block gives. A run whose frame reads stalled
//! gets the page `yunta status` prints instead, since no block closes it out.
//!
//! A follower only reads: it takes no lock, writes no event, writes no registry and
//! never signals the engine. A run that is not moving has nothing to follow, so it gets
//! the page `yunta status` prints and the code its stop gives; but one that closed
//! promoted before it is followed is followed on to its successor, read whole, once the
//! following hands one, and where none comes in the time one takes to be created since
//! the close, nothing moved while it was followed, and its page is what is printed.
//! Ctrl-C stops the follower alone: the run goes on in the process that drives it.

use surco::look::Look;
use surco::words::Standing;
use yunta_api::{FollowNews, Followed, FollowedRun, Following, Project, RunNews, Stop};
use yunta_core::events::StoredEvent;
use yunta_core::RunId;
use yunta_render::page::LeftRunning;

use crate::commands::drive::{report_closing, Closed};
use crate::error::CliError;
use crate::output::{Mode, Outcome};
use crate::shell::Shell;
use crate::surface::{Delivery, EngineReading, Surface, SurfaceEnv, TerminalEnv};

/// Follows `run_id`, and answers with the code its stop gives.
pub(crate) async fn follow(
    shell: &Shell,
    run_id: &RunId,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let followed = Followed {
        field: None,
        run: Some(run_id.clone()),
    };
    let mut following = project.follow(followed).await?;
    let stop = shell.stop();
    match start(&mut following, &stop, &project).await {
        Ok(Start::Moving(run, events)) => {
            let reading = (&project, look);
            moving(&mut following, &stop, *run, events, reading).await
        }
        Ok(Start::Still(still)) => page(&project, &still, look).await,
        Err(yunta_api::Error::Stopped) => Ok(left_running(run_id, look)),
        Err(error) => Err(error.into()),
    }
}

/// `kept`, a run that moves, read whole with `events`, followed on the surface `yunta
/// run` draws until the rule every interface stops a following by: the block it leaves,
/// or the page of a run whose engine went, and the code either gives.
async fn moving(
    following: &mut Following,
    stop: &Stop,
    mut kept: FollowedRun,
    events: Vec<StoredEvent>,
    (project, look): (&Project, &Look),
) -> Result<Outcome, CliError> {
    let run_id = &kept.run.clone();
    let clock = std::sync::Arc::clone(&project.workspace.clock);
    let frame = kept.frame(clock.now());
    let started = events.first().map(|event| event.timestamp);
    let prior = project
        .prior_estimation(&kept.manifest, &frame.mode, started)
        .await;
    let log = project.event_log().await?;
    let registry = project.run(run_id).await?.registry();
    let surface = Surface::open(
        SurfaceEnv {
            run_id,
            manifest: &kept.manifest,
            prior: prior.as_ref(),
            log: &log,
            clock: std::sync::Arc::clone(&clock),
            delivery: Delivery::choose(false, TerminalEnv::settled()),
            glyphs: crate::output::glyphs(),
            engine: EngineReading::Registry(registry),
        },
        events,
    );
    let drawn = drawn(following, &mut kept, stop, &surface, (project, look)).await;
    surface.close().await;
    match drawn {
        // What drove the run is gone, so no block closes it out: the page says
        // what holds it.
        Ok(()) if kept.frame(clock.now()).standing == Standing::Stalled => {
            page(project, run_id, look).await
        }
        Ok(()) => closing(&kept, prior.as_ref(), project, look).await,
        Err(yunta_api::Error::Stopped) => Ok(left_running(run_id, look)),
        Err(error) => Err(error.into()),
    }
}

/// The page `yunta status` prints of `run`, and the code its stop gives.
async fn page(project: &Project, run: &RunId, look: &Look) -> Result<Outcome, CliError> {
    let standing = super::print_page(project, run, look).await?;
    Ok(crate::output::exit(standing))
}

/// Where a follower starts. Not where it stops, which the rule every interface stops a
/// following by says.
enum Start {
    /// The run that moves, read whole, with every event of its log from its first.
    Moving(Box<FollowedRun>, Vec<StoredEvent>),
    /// The run that stands still where what was read ends: nothing moved while it was
    /// followed, so its page is all there is to show.
    Still(RunId),
}

/// Where a follower of `following` starts, at `project`'s clock: the run read whole,
/// where it moves; else, where it closed promoted, its successor read whole once one is
/// handed, on along the chain; else the run that stands still.
async fn start(
    following: &mut Following,
    stop: &Stop,
    project: &Project,
) -> Result<Start, yunta_api::Error> {
    let (mut run, mut events) = read_whole(following, stop).await?;
    while !run.frame(project.workspace.clock.now()).moves() {
        let Some(successor) = handed_on(following, stop).await? else {
            return Ok(Start::Still(run.run));
        };
        (run, events) = successor;
    }
    Ok(Start::Moving(Box::new(run), events))
}

/// The run `following` hands first, read whole, and every event of its log from its
/// first.
async fn read_whole(
    following: &mut Following,
    stop: &Stop,
) -> Result<(FollowedRun, Vec<StoredEvent>), yunta_api::Error> {
    loop {
        // A following hands the run it was asked for read whole before anything of it.
        if let FollowNews::Whole { run, events } = following.next(stop).await? {
            return Ok((*run, events));
        }
    }
}

/// The successor `following` hands, read whole with every event of its log, while it
/// awaits one; none once it hands that none came, or where it awaits none. What else it
/// hands of a run that stands still shows nothing a page does not.
async fn handed_on(
    following: &mut Following,
    stop: &Stop,
) -> Result<Option<(FollowedRun, Vec<StoredEvent>)>, yunta_api::Error> {
    while following.awaits_successor() {
        if let FollowNews::Run {
            news: RunNews::Successor { run, events },
            ..
        } = following.next(stop).await?
        {
            return Ok(Some((*run, events)));
        }
    }
    Ok(None)
}

/// What a follower stopped by its person says: the run goes on in the process that
/// drives it.
fn left_running(run_id: &RunId, look: &Look) -> Outcome {
    crate::output::show(look, &LeftRunning { run_id });
    crate::output::Outcome::interrupted()
}

/// Hands `surface` each news of `following` until the run's frame, as `kept` reads it
/// at `project`'s clock with each batch folded in, no longer moves and the following
/// awaits no successor: the events, and what was said through its observer, the same
/// door the engine of `yunta run`'s own process hands it through, so the region does not
/// know which feed a frame came by. A successor's log moves the region onto it. Where
/// the engine stands the surface reads from the registry itself. What reading a run
/// remarks on is said through the CLI's remarks with `look`, as `yunta status` says it,
/// never on the surface.
async fn drawn(
    following: &mut Following,
    kept: &mut FollowedRun,
    stop: &Stop,
    surface: &Surface,
    (project, look): (&Project, &Look),
) -> Result<(), yunta_api::Error> {
    while following.awaits_successor() || kept.frame(project.workspace.clock.now()).moves() {
        // Only the run asked for is followed, so every piece of news is of the run kept.
        let FollowNews::Run { news, .. } = following.next(stop).await? else {
            continue;
        };
        kept.took(&news);
        match news {
            RunNews::Events(events) | RunNews::Successor { events, .. } => {
                for event in events {
                    surface.deliver(event).await;
                }
            }
            RunNews::Said { said, .. } => {
                if let Some(ear) = surface.observer() {
                    ear.said(&said);
                }
            }
            RunNews::Remark(remark) => crate::remarks::say(remark, Mode::Human, look),
            // What the run waits on a person for is the CLI's prompts' to read, and what
            // its nodes hand over is the node's page's.
            RunNews::Engine(_)
            | RunNews::NoSuccessor
            | RunNews::Awaited(_)
            | RunNews::AwaitedUnread { .. }
            | RunNews::Document { .. } => {}
        }
    }
    Ok(())
}

/// The block `yunta run` leaves when the run followed last stops, read from the
/// log, and the code it gives.
async fn closing(
    followed: &FollowedRun,
    prior: Option<&yunta_replay::history::PriorEstimation>,
    project: &Project,
    look: &Look,
) -> Result<Outcome, CliError> {
    let (run_id, manifest) = (&followed.run, &followed.manifest);
    let run_dir = project.run(run_id).await?.dir;
    let bound = followed
        .state
        .run
        .checkout()
        .map(std::path::Path::to_path_buf);
    let here = &project.workspace.dir;
    let worktree = project.run_tree(manifest, bound.as_deref(), run_id, here);
    let reading = project.closing(run_id, &run_dir, manifest).await?;
    let closed = Closed {
        run_id,
        manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        prior,
        cwd: &project.workspace.dir,
        home: project.workspace.env.home.as_deref(),
    };
    Ok(report_closing(&closed.closing(reading), look))
}

//! `yunta status <run> --follow`: a run followed from a process that did not
//! start it.
//!
//! The follower follows the run through the API's [`Following`], which reads the
//! run's log once and then hands back what comes, at one pace for every interface
//! that follows a run: the events past the last it read, what the run's sessions
//! say, the successor a promotion hands the work to, and where the run ends. The
//! follower opens the surface `yunta run` draws with what was read, hands it each
//! event and what was said, and when the run stops takes the surface down and
//! prints the block `yunta run` leaves, read from the log the same way, and exits
//! with the code that block gives.
//!
//! A follower only reads: it takes no lock, writes no event, writes no registry and
//! never signals the engine. A run that is not moving has nothing to follow, so it
//! gets the page `yunta status` prints and the code its stop gives. Ctrl-C stops
//! the follower alone: the run goes on in the process that drives it.

use surco::look::Look;
use yunta_api::{Following, Project, RunNews, Stop};
use yunta_core::RunId;
use yunta_render::page::LeftRunning;

use crate::commands::drive::{report_closing, Closed};
use crate::error::CliError;
use crate::output::Outcome;
use crate::shell::Shell;
use crate::surface::{Delivery, EngineReading, Surface, SurfaceEnv, TerminalEnv};

/// Follows `run_id`, and answers with the code its stop gives.
pub(crate) async fn follow(
    shell: &Shell,
    run_id: &RunId,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let mut following = project.follow(run_id).await?;
    let frame = following.frame();
    if !frame.moves() {
        let standing = super::print_page(&project, run_id, look).await?;
        return Ok(crate::output::exit(standing));
    }
    let followed = following.followed();
    let started = followed.events.first().map(|event| event.timestamp);
    let prior = project
        .prior_estimation(&followed.manifest, &frame.mode, started)
        .await;
    let surface = Surface::open(
        SurfaceEnv {
            run_id,
            manifest: &followed.manifest,
            prior: prior.as_ref(),
            log: following.log(),
            clock: std::sync::Arc::clone(&project.workspace.clock),
            delivery: Delivery::choose(false, TerminalEnv::settled()),
            glyphs: crate::output::glyphs(),
            engine: EngineReading::Registry(following.registry().clone()),
        },
        followed.events.clone(),
    );
    let ended = drawn(&mut following, &shell.stop(), &surface).await;
    surface.close().await;
    match ended {
        Ok(Ended::Stopped) => closing(&following, prior.as_ref(), &project, look).await,
        // What drove the run is gone, so no block closes it out: the page says
        // what holds it.
        Ok(Ended::Stalled) => {
            let standing = super::print_page(&project, run_id, look).await?;
            Ok(crate::output::exit(standing))
        }
        Err(yunta_api::Error::Stopped) => {
            crate::output::show(look, &LeftRunning { run_id });
            Ok(crate::output::Outcome::interrupted())
        }
        Err(error) => Err(error.into()),
    }
}

/// How following a run ended, with the run still followed.
enum Ended {
    /// The run is no longer moving.
    Stopped,
    /// Its log says it moves, and the engine its registry names is gone.
    Stalled,
}

/// Hands `surface` each news of `following` until the run stops: the events, and
/// what was said through its observer, the same door the engine of `yunta run`'s own
/// process hands it through, so the region does not know which feed a frame came by.
/// A successor's log moves the region onto it. Where the engine stands the surface
/// reads from the registry itself.
async fn drawn(
    following: &mut Following,
    stop: &Stop,
    surface: &Surface,
) -> Result<Ended, yunta_api::Error> {
    let mut ended = Ended::Stopped;
    while let Some(news) = following.next(stop).await? {
        match news {
            RunNews::Events(events) => {
                for event in events {
                    surface.deliver(event).await;
                }
            }
            RunNews::Successor(successor) => {
                for event in successor.events {
                    surface.deliver(event).await;
                }
            }
            RunNews::Said(said) => {
                if let Some(ear) = surface.observer() {
                    ear.said(&said);
                }
            }
            RunNews::Engine(_) => {}
            RunNews::Stopped => ended = Ended::Stopped,
            RunNews::Stalled => ended = Ended::Stalled,
        }
    }
    Ok(ended)
}

/// The block `yunta run` leaves when the run followed last stops, read from the
/// log, and the code it gives.
async fn closing(
    following: &Following,
    prior: Option<&yunta_replay::history::PriorEstimation>,
    project: &Project,
    look: &Look,
) -> Result<Outcome, CliError> {
    let followed = following.followed();
    let (run_id, manifest, run_dir) = (&followed.run, &followed.manifest, following.dir());
    let bound = followed
        .state
        .run
        .checkout()
        .map(std::path::Path::to_path_buf);
    let here = &project.workspace.dir;
    let worktree = project.run_tree(manifest, bound.as_deref(), run_id, here);
    let reading = project.closing(run_id, run_dir, manifest).await?;
    let closed = Closed {
        run_id,
        manifest,
        run_dir,
        worktree: &worktree,
        prior,
        cwd: &project.workspace.dir,
        home: project.workspace.env.home.as_deref(),
    };
    Ok(report_closing(&closed.closing(reading), look))
}

//! Driving a run here, as this invocation shows it.
//!
//! One path, taken by both commands that execute a run: `yunta run` after it
//! creates one, `yunta resume` after it takes one back. The API drives the
//! run; what this adds is how the invocation shows it — the live surface and
//! the console a gate asks at while the run moves, and the block, the verdict
//! line or the document that closes it out — so a stop reported here reads
//! the same whichever command reached it.

mod watch;

use std::path::Path;

use surco::glyphs::Glyphs;
use surco::look::Look;
use yunta_api::{ClosingReading, Driven, HeldRun, Ports, Remark};
use yunta_core::{Manifest, RunId};
use yunta_render::page::Places;
use yunta_replay::history::PriorEstimation;

use crate::context::Context;
use crate::error::CliError;
use crate::output::Mode;
use crate::output::Outcome;
use crate::surface::{Closing, ClosingEnv, Delivery, TerminalEnv};
use watch::watch;

/// What the invocation that drives a run here reports with: where it runs,
/// how it reports — a live surface for `Human`, the run id and one verdict
/// line for `Quiet`, one versioned document with no live surface at all for
/// `Json` — and the stdout look everything it draws is drawn with.
pub(crate) struct Driving<'a> {
    pub(crate) ctx: &'a Context,
    pub(crate) output: Mode,
    pub(crate) look: &'a Look,
}

/// How this invocation shows the run — derived here rather than handed in,
/// so no caller can present the same stop a different way.
struct Presentation {
    delivery: Delivery,
    glyphs: Glyphs,
}

impl Presentation {
    /// What `--quiet` and the environment this process was handed allow.
    fn of(output: Mode) -> Self {
        Self {
            delivery: Delivery::choose(output == Mode::Quiet, TerminalEnv::settled()),
            glyphs: crate::output::glyphs(),
        }
    }
}

/// Drives `held` here under a live surface and closes the invocation out:
/// the one path `yunta run` and `yunta resume` both take once the run they
/// drive is held.
pub(crate) async fn drive(env: Driving<'_>, held: HeldRun<'_>) -> Result<Outcome, CliError> {
    let shown = Presentation::of(env.output);
    let watching = watch(&env, &held, &shown).await?;
    let stop = env.ctx.stop();
    let (output, look) = (env.output, env.look);
    let say = |remark: Remark| crate::remarks::say(remark, output, look);
    let ports = Ports {
        person: &watching.asking,
        observer: watching.observer.clone(),
    };
    let driven = held.drive(ports, &say, &stop).await;
    watching.close().await;
    let outcome = reported(&env, &driven?).await?;
    // A person who interrupted the invocation is told so by its exit code,
    // as a shell tells it of any process SIGINT ended, whatever the run
    // reached on its way out.
    Ok(match stop.is_stopped() {
        true => Outcome::interrupted(),
        false => outcome,
    })
}

/// How the run ended, said the way this invocation was asked to say it: the
/// document, one verdict line, or the block that closes the run out.
async fn reported(env: &Driving<'_>, driven: &Driven<'_>) -> Result<Outcome, CliError> {
    if env.output == Mode::Json {
        let document = driven.document().await?;
        crate::output::print_json(&document)?;
        return Ok(crate::output::exit(document.outcome()));
    }
    let closed = Closed {
        run_id: &driven.id,
        manifest: &driven.manifest,
        run_dir: &driven.dir,
        worktree: &driven.worktree,
        prior: driven.prior.as_ref(),
        cwd: &env.ctx.cwd,
        home: env.ctx.env.home.as_deref(),
    };
    let closing = closed.closing(driven.closing().await?);
    // The run id already went out when the invocation opened. The verdict
    // line and the exit code come from the one reading the closing block
    // takes.
    Ok(match env.output {
        Mode::Quiet => report_verdict(&closing),
        _ => report_closing(&closing, env.look),
    })
}

/// A run that stopped, as the block that closes it names it.
pub(crate) struct Closed<'a> {
    pub(crate) run_id: &'a RunId,
    /// The run's own frozen manifest — what says which workflow it ran,
    /// which branch it started from, and how it isolated its tree.
    pub(crate) manifest: &'a Manifest,
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
    /// What this workflow's past runs cost, for the block's own comparison;
    /// `None` below the history floor.
    pub(crate) prior: Option<&'a PriorEstimation>,
    /// Where paths are shown from: the directory this was run in, and the
    /// home `~` stands for.
    pub(crate) cwd: &'a Path,
    pub(crate) home: Option<&'a Path>,
}

impl Closed<'_> {
    /// The block that closes the run out, from what its log says once it
    /// stopped.
    pub(crate) fn closing(&self, reading: ClosingReading) -> Closing {
        Closing::of(ClosingEnv {
            run_id: self.run_id,
            workflow: &self.manifest.workflow,
            state: &reading.state,
            prior: self.prior,
            now: reading.now,
            decision: reading.decision,
            shown: reading.shown,
            places: Places {
                run_dir: self.run_dir,
                worktree: self.worktree,
                branch: &self.run_id.branch(),
                base_branch: &self.manifest.base_branch,
                isolation: self.manifest.isolation,
                cwd: self.cwd,
                home: self.home,
            },
        })
    }
}

/// The one line a quiet run says on stderr: what its closing page opens
/// with, and the verdict the exit code carries.
pub(crate) fn report_verdict(closing: &Closing) -> Outcome {
    let verdict = surco::doc::Doc::new().with(closing.page().headline());
    let look = crate::output::stderr_look();
    crate::output::note(crate::output::draw(verdict, &look).trim_end());
    closing.outcome()
}

/// Prints the block that closes the run out, and hands back the verdict the
/// exit code carries.
pub(crate) fn report_closing(closing: &Closing, look: &Look) -> Outcome {
    crate::output::show(look, &closing.page());
    closing.outcome()
}

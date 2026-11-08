//! What a run leaves on the terminal once it stops: the run's page in its
//! harvest form, drawn by [`RunPage`].
//!
//! The page says how the run ended and what holds it, the evidence of
//! what failed, the facts of the harvest — what it took, where its work
//! is, the runs it composed, each under the node that bore it and never
//! averaged into a figure of its own — and last the decision a parked run
//! waits on, or what to type next.
//!
//! The mark beside the word only repeats it. A run that finished promoted,
//! cancelled, broken, or carrying blocking findings never gets the mark a
//! clean finish gets — a tick over work nobody has accepted is the one
//! decoration that would say something the word does not.
//!
//! This module gathers what the page reads from a stopped run.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use surco::doc::Depth;
use yunta_core::events::DecisionOn;
use yunta_core::events::GateWaitingPayload;
use yunta_core::shown::ShownDocument;
use yunta_core::{Isolation, RunId, Workflow};
use yunta_render::escalation::Decision;
use yunta_replay::frame::RunFrame;
use yunta_replay::history::PriorEstimation;
use yunta_replay::RunState;

use crate::error::Outcome;
use crate::run_page::{RunPage, Scope};

/// Where the run lived and what it took — what the closing block reports
/// that the run's own log does not carry.
#[derive(Clone, Copy)]
pub(crate) struct Outline<'a> {
    pub(crate) run_dir: &'a Path,
    pub(crate) worktree: &'a Path,
    /// The branch the run started from, as its manifest froze it.
    pub(crate) base_branch: &'a str,
    pub(crate) isolation: Isolation,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub(crate) cwd: &'a Path,
    pub(crate) home: Option<&'a Path>,
}

/// Everything the block needs, gathered from the log as it finally stands.
pub(crate) struct ClosingEnv<'a> {
    pub(crate) run_id: &'a RunId,
    pub(crate) workflow: &'a Workflow,
    /// The run as its log derives it, once the run has stopped.
    pub(crate) state: &'a RunState,
    pub(crate) prior: Option<&'a PriorEstimation>,
    pub(crate) now: DateTime<Utc>,
    /// The decision a parked run stopped on, rebuilt from the manifest and
    /// the log. `None` for a pause with no menu to rebuild, and for a run
    /// that is not parked at all.
    pub(crate) decision: Option<(DecisionOn, GateWaitingPayload)>,
    /// The documents that decision shows, as the run holds them.
    pub(crate) shown: Vec<ShownDocument>,
    pub(crate) outline: Outline<'a>,
}

/// A stopped run as a person reads it. It owns what its page is drawn from,
/// so a page can borrow it for as long as a document needs.
pub(crate) struct Closing {
    run_id: RunId,
    state: RunState,
    frame: RunFrame,
    decision: Option<(DecisionOn, GateWaitingPayload)>,
    shown: Vec<ShownDocument>,
    run_dir: PathBuf,
    worktree: PathBuf,
    base_branch: String,
    isolation: Isolation,
    cwd: PathBuf,
    home: Option<PathBuf>,
}

impl Closing {
    /// Builds the block from the run's own state — the truth about what
    /// happened, read once after the run stops rather than carried along
    /// from whatever the surface managed to see.
    pub(crate) fn of(env: ClosingEnv<'_>) -> Self {
        // The block is drawn by the process that drove the run.
        let frame = RunFrame::of(
            env.run_id,
            env.workflow,
            env.state,
            env.prior,
            &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
            env.now,
        );
        Self::framed(
            env.run_id,
            frame,
            env.state.clone(),
            (env.decision, env.shown),
            &env.outline,
        )
    }

    /// The block for `frame`, as the run's log derived it.
    fn framed(
        run_id: &RunId,
        frame: RunFrame,
        state: RunState,
        (decision, shown): (Option<(DecisionOn, GateWaitingPayload)>, Vec<ShownDocument>),
        outline: &Outline<'_>,
    ) -> Self {
        Self {
            run_id: run_id.clone(),
            state,
            frame,
            decision,
            shown,
            run_dir: outline.run_dir.to_path_buf(),
            worktree: outline.worktree.to_path_buf(),
            base_branch: outline.base_branch.to_string(),
            isolation: outline.isolation,
            cwd: outline.cwd.to_path_buf(),
            home: outline.home.map(Path::to_path_buf),
        }
    }

    /// Whether the invocation reports success. Only a run that finished
    /// does: a paused, failed, cancelled or promoted run ran to a stop
    /// that needs a decision, and its detail is already on this block.
    pub(crate) fn outcome(&self) -> Outcome {
        crate::render::exit(self.frame.standing)
    }

    /// The page this block is drawn from.
    pub(crate) fn page(&self) -> RunPage<'_> {
        RunPage {
            scope: Scope::Closing,
            depth: Depth::Glance,
            run_id: &self.run_id,
            frame: &self.frame,
            state: &self.state,
            paths: Outline {
                run_dir: &self.run_dir,
                worktree: &self.worktree,
                base_branch: &self.base_branch,
                isolation: self.isolation,
                cwd: &self.cwd,
                home: self.home.as_deref(),
            },
            decision: self.decision.as_ref().map(|(on, escalation)| Decision {
                on,
                escalation,
                shown: &self.shown,
            }),
            plans: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;

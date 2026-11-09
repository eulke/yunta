//! What a run leaves on the terminal once it stops: the run's page in its
//! harvest form, render's `page::Run` in its closing scope.
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

use crate::output::Outcome;
use yunta_render::page::{Places, Run, RunScope};

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
    pub(crate) places: Places<'a>,
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
    branch: String,
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
            &env.places,
        )
    }

    /// The block for `frame`, as the run's log derived it.
    fn framed(
        run_id: &RunId,
        frame: RunFrame,
        state: RunState,
        (decision, shown): (Option<(DecisionOn, GateWaitingPayload)>, Vec<ShownDocument>),
        places: &Places<'_>,
    ) -> Self {
        Self {
            run_id: run_id.clone(),
            state,
            frame,
            decision,
            shown,
            run_dir: places.run_dir.to_path_buf(),
            worktree: places.worktree.to_path_buf(),
            branch: places.branch.to_string(),
            base_branch: places.base_branch.to_string(),
            isolation: places.isolation,
            cwd: places.cwd.to_path_buf(),
            home: places.home.map(Path::to_path_buf),
        }
    }

    /// Whether the invocation reports success. Only a run that finished
    /// does: a paused, failed, cancelled or promoted run ran to a stop
    /// that needs a decision, and its detail is already on this block.
    pub(crate) fn outcome(&self) -> Outcome {
        crate::output::exit(self.frame.standing)
    }

    /// The page this block is drawn from.
    pub(crate) fn page(&self) -> Run<'_> {
        Run {
            scope: RunScope::Closing {
                exit_code: self.outcome().code(),
            },
            depth: Depth::Glance,
            run_id: &self.run_id,
            frame: &self.frame,
            state: &self.state,
            places: Places {
                run_dir: &self.run_dir,
                worktree: &self.worktree,
                branch: &self.branch,
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

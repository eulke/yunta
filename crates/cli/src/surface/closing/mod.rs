//! What a run leaves on the terminal once it stops.
//!
//! In the order a person reads it for, as `yunta status` prints a run:
//! the outcome as a word and what holds the run, the evidence of what
//! failed, the decision a parked run waits on, the facts that have
//! something to say, and what to type next. A fact with nothing to say
//! is not a row.
//!
//! The mark beside the word only repeats it. A run that finished promoted,
//! cancelled, broken, or carrying blocking findings never gets the mark a
//! clean finish gets — a tick over work nobody has accepted is the one
//! decoration that would say something the word does not.
//!
//! A run that composed others closes with them under it as a tree, each
//! child under the node that bore it and never averaged into a figure of
//! its own.
//!
//! The page is drawn by [`RunPage`]. This module gathers what the page
//! reads from a stopped run, and holds the sections only a closing shows.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::GateWaitingPayload;
use yunta_core::text::counted;
use yunta_core::{Isolation, NodeId, RunId, Workflow};
use yunta_engine::{
    run_frame_of, NodeFrame, PriorEstimation, RunFrame, RunPhase, RunState, ShownDocument,
};

use crate::commands::status::decision;
use crate::commands::{advice, unknown_kinds_note};
use crate::error::Outcome;
use crate::render::blocks::{Drawn, FailureDetail, Fields, Next, Whole};
use crate::render::ink::{Line, Tone};
use crate::render::{duration, indent, paths, wrap, Glyphs, Look, INDENT};
use crate::run_page::{caution_row, failed, Decided, RunPage, Scope};

use super::view;

/// How many of the run's longest nodes the block names. Enough to point
/// at where the time went, few enough that the row stays one row.
const SLOWEST: usize = 3;

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
    pub(crate) decision: Option<(NodeId, GateWaitingPayload)>,
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
    decision: Option<(NodeId, GateWaitingPayload)>,
    shown: Vec<ShownDocument>,
    run_dir: PathBuf,
    worktree: PathBuf,
    base_branch: String,
    isolation: Isolation,
    cwd: PathBuf,
    home: Option<PathBuf>,
    /// The process that drove the run is this one, which asks no one.
    engine: crate::render::Engine,
}

impl Closing {
    /// Builds the block from the run's own state — the truth about what
    /// happened, read once after the run stops rather than carried along
    /// from whatever the surface managed to see.
    pub(crate) fn of(env: ClosingEnv<'_>) -> Self {
        let frame = run_frame_of(env.run_id, env.workflow, env.state, env.prior, env.now);
        Self::framed(
            env.run_id,
            frame,
            env.state.clone(),
            env.now,
            (env.decision, env.shown),
            &env.outline,
        )
    }

    /// The block for `frame`, as the run's log derived it.
    fn framed(
        run_id: &RunId,
        frame: RunFrame,
        state: RunState,
        now: DateTime<Utc>,
        (decision, shown): (Option<(NodeId, GateWaitingPayload)>, Vec<ShownDocument>),
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
            engine: crate::render::Engine::this(now),
        }
    }

    /// Whether the invocation reports success. Only a run that finished
    /// does: a paused, failed, cancelled or promoted run ran to a stop
    /// that needs a decision, and its detail is already on this block.
    pub(crate) fn outcome(&self) -> Outcome {
        crate::render::exit(crate::render::run_word(&self.frame))
    }

    /// The page this block is drawn from. The decision it waits on is laid
    /// out for `look`, the look the page is drawn with.
    pub(crate) fn page(&self, look: &Look) -> RunPage<'_> {
        RunPage {
            scope: Scope::Closing,
            run_id: &self.run_id,
            engine: self.engine.clone(),
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
            decision: self.decided(look),
        }
    }

    /// The decision a parked run stopped on: every option with the command
    /// that chooses it, the claim and the record the page does not already
    /// say, and where the run holds its state.
    fn decided(&self, look: &Look) -> Option<Decided> {
        let (node, escalation) = self.decision.as_ref()?;
        let said_why = match &self.frame.phase {
            RunPhase::Waiting { on } => advice::parked_says_why(on),
            _ => false,
        };
        let beside = decision::Beside {
            claim: true,
            evidence: said_why
                || failed(&self.frame).any(|(failed_node, _)| failed_node.id == *node),
        };
        let mut lines =
            decision::lines((&self.run_id, node), escalation, &self.shown, beside, look);
        // A person who has just watched their terminal stop is the one
        // who needs telling that nothing holds the answer open.
        let room = look.width.cells().saturating_sub(INDENT.len());
        lines.extend(
            wrap(
                "the run holds its own state on disk — close this terminal whenever you \
                 like and answer from anywhere.",
                room,
            )
            .into_iter()
            .map(|part| Line::new().plain(INDENT).push(Tone::Muted, part)),
        );
        Some(Decided {
            lines,
            escalation: Some(escalation.clone()),
        })
    }
}

impl RunPage<'_> {
    /// Every failed node with what its failure says beyond its claim: the
    /// end of what a command printed and where the rest is kept, each
    /// document it refused, the paths it should not have touched.
    ///
    /// A node whose failure has nothing more to quote, and whose claim the
    /// line under the outcome already made, is not listed again.
    pub(crate) fn closing_evidence(&self, look: &Look) -> Vec<Line> {
        let held = self
            .closing_said()
            .map(|(_, said)| said)
            .unwrap_or_default();
        let mut lines = Vec::new();
        for (node, failure) in failed(self.frame) {
            let whole = failure.output().map(|output| {
                Whole::file(
                    yunta_engine::ObjectStore::at(self.paths.run_dir).path_of(output),
                    self.paths.cwd,
                    self.paths.home,
                )
            });
            let detail = FailureDetail { failure, whole }.lines(look);
            if detail.is_empty() && held.contains(&format!("`{}`", node.id)) {
                continue;
            }
            lines.push(
                Line::new()
                    .plain(INDENT)
                    .push(Tone::Strong, node.id.as_str())
                    .plain(": ")
                    .push(Tone::Failed, yunta_render::failure::headline(failure)),
            );
            lines.extend(detail.into_iter().map(|line| line.under(INDENT)));
        }
        lines
    }

    /// The facts a reader checks once they know how the run stopped, and
    /// the runs it composed, each under the node that bore it.
    pub(crate) fn closing_facts(&self, look: &Look) -> Vec<Line> {
        let mut lines = self.closing_fields(look.glyphs).lines(look);
        lines.extend(self.closing_children(look));
        lines
    }

    /// The facts with something to say, in the order a reader checks them.
    fn closing_fields(&self, glyphs: Glyphs) -> Fields {
        let degraded = match self.frame.degraded.len() {
            0 => String::new(),
            n => counted(n, "capability the adapter lacks"),
        };
        Fields::new()
            .push_if("progress", crate::render::counter::line(self.frame, glyphs))
            .push_if("tokens", self.closing_tokens(glyphs))
            .push_if("caution", caution_row(self.frame))
            .push_if("slowest", self.slowest(glyphs).unwrap_or_default())
            .push_if("branch", self.branch())
            .push_if("artifacts", self.artifacts())
            .push_if("degraded", degraded)
            .push_if(
                "unread",
                unknown_kinds_note(&self.frame.unknown_kinds, glyphs).unwrap_or_default(),
            )
    }

    /// What the run spent, and what this workflow's own past runs spent —
    /// the only honest comparison there is, because it is what already
    /// happened rather than a prediction. Nothing when the run spent
    /// nothing and has no history to compare with.
    fn closing_tokens(&self, glyphs: Glyphs) -> String {
        match &self.frame.prior {
            Some(prior) => format!(
                "{} {} {}",
                yunta_render::spend::Breakdown(&self.frame.tokens),
                glyphs.sep(),
                history(prior, glyphs)
            ),
            None => self.tokens().unwrap_or_default(),
        }
    }

    /// The runs this one composed, as a tree: every child under the node
    /// that bore it.
    ///
    /// A tree and never an average, because averaging heterogeneous
    /// children is the lying percentage under another name. It closes
    /// the facts rather than opening them: a child is a run of its own,
    /// with its own id to go and read, and what this block is for is the
    /// run it closes. Empty for a run that composed nothing, which is
    /// most of them.
    fn closing_children(&self, look: &Look) -> Vec<Line> {
        if self.frame.children.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![Line::new().plain(INDENT).push(Tone::Muted, "children")];
        for (under, born) in view::children_by_node(self.frame) {
            lines.push(Line::new().plain(indent(2)).plain(under));
            for child in born {
                lines.push(
                    Line::new()
                        .plain(indent(3))
                        .plain(view::child_row(child, look.glyphs)),
                );
            }
        }
        lines
    }

    /// The run's longest nodes, longest first. `None` for a run where no
    /// node ever started.
    fn slowest(&self, glyphs: Glyphs) -> Option<String> {
        let mut timed: Vec<(&NodeFrame, Duration)> = self
            .frame
            .nodes
            .iter()
            .filter_map(|node| node.elapsed.map(|elapsed| (node, elapsed)))
            .collect();
        if timed.is_empty() {
            return None;
        }
        timed.sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));
        Some(
            timed
                .iter()
                .take(SLOWEST)
                .map(|(node, elapsed)| format!("{} {}", node.id, duration(*elapsed)))
                .collect::<Vec<_>>()
                .join(&format!(" {} ", glyphs.sep())),
        )
    }

    /// The branch the run worked on and where that work sits now.
    fn branch(&self) -> String {
        let tree = paths::shown(self.paths.worktree, self.paths.cwd, self.paths.home);
        match self.paths.isolation {
            Isolation::Worktree => format!(
                "{} off {}, in {tree}",
                yunta_engine::run_branch(self.run_id),
                self.paths.base_branch,
            ),
            Isolation::None => format!("{}, in this checkout at {tree}", self.paths.base_branch),
        }
    }

    /// How many artifacts the run's nodes wrote, and the one directory
    /// they are under. Nothing for a run that wrote none.
    fn artifacts(&self) -> String {
        let written: usize = self
            .frame
            .nodes
            .iter()
            .map(|node| node.artifacts.len())
            .sum();
        if written == 0 {
            return String::new();
        }
        format!(
            "{} under {}",
            counted(written, "artifact"),
            paths::shown(
                &yunta_engine::run_dir::artifacts_view(self.paths.run_dir),
                self.paths.cwd,
                self.paths.home
            )
        )
    }

    /// What a person types next, given how this run stopped, and where
    /// to read the whole of it.
    pub(crate) fn closing_next(&self) -> Next {
        let handle = self.run_id.handle();
        let mut steps = advice::after(
            crate::render::run_word(self.frame),
            handle,
            self.decided_escalation(),
        );
        steps.push((advice::status(handle), "shows the whole run"));
        Next { steps }
    }
}

/// What this workflow's past runs cost, in the one phrasing every surface
/// that reports it uses.
fn history(prior: &PriorEstimation, glyphs: Glyphs) -> String {
    crate::commands::stats::format_estimation_line(prior, glyphs)
}

#[cfg(test)]
mod tests;

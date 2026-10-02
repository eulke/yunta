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

use crate::render::state::RunWord;
use std::path::{Path, PathBuf};
use std::time::Duration;

use yunta_core::events::{GateWaitingPayload, NodeState, StoredEvent};
use yunta_core::{Isolation, NodeId, RunId, Workflow};
use yunta_engine::{run_frame, NodeFrame, NodeStanding, PriorEstimation, RunFrame, RunPhase};

use crate::commands::status::decision;
use crate::commands::{advice, unknown_kinds_note};
use crate::error::Outcome;
use crate::render::blocks::{Block, FailureDetail, Fields, Headline, Next, Whole};
use crate::render::ink::{Line, Tone};
use crate::render::{duration, indent, paths, wrap, Glyphs, Look, Tokens, INDENT};
use yunta_core::text::counted;

use super::view;

/// How many of the run's longest nodes the block names. Enough to point
/// at where the time went, few enough that the row stays one row.
const SLOWEST: usize = 3;

/// Where the run lived and what it took — what the closing block reports
/// that the run's own log does not carry.
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
    pub(crate) events: &'a [StoredEvent],
    pub(crate) prior: Option<&'a PriorEstimation>,
    pub(crate) now: chrono::DateTime<chrono::Utc>,
    /// The decision a parked run stopped on, rebuilt from the manifest and
    /// the log. `None` for a pause with no menu to rebuild, and for a run
    /// that is not parked at all.
    pub(crate) decision: Option<(NodeId, GateWaitingPayload)>,
    pub(crate) outline: Outline<'a>,
}

/// A stopped run as a person reads it.
pub(crate) struct Closing {
    run_id: RunId,
    frame: RunFrame,
    decision: Option<(NodeId, GateWaitingPayload)>,
    run_dir: PathBuf,
    worktree: PathBuf,
    base_branch: String,
    isolation: Isolation,
    cwd: PathBuf,
    home: Option<PathBuf>,
}

impl Closing {
    /// Derives the block from the run's own log — the truth about what
    /// happened, read once after the run stops rather than carried along
    /// from whatever the surface managed to see.
    pub(crate) fn of(env: ClosingEnv<'_>) -> Self {
        let frame = run_frame(env.run_id, env.workflow, env.events, env.prior, env.now);
        Self::framed(env.run_id, frame, env.decision, &env.outline)
    }

    /// The block for `frame`, as the run's log derived it.
    fn framed(
        run_id: &RunId,
        frame: RunFrame,
        decision: Option<(NodeId, GateWaitingPayload)>,
        outline: &Outline<'_>,
    ) -> Self {
        Self {
            run_id: run_id.clone(),
            frame,
            decision,
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
        RunWord::of(&self.frame).exit()
    }

    /// The whole block, ready to print on a stream with `look`.
    pub(crate) fn render(&self, look: &Look) -> String {
        let paint = |lines: Vec<Line>| -> String {
            lines
                .iter()
                .map(|line| format!("{}\n", look.ink.paint(line)))
                .collect()
        };
        let mut out = paint(self.head(look));
        let mut facts = self.fields(look.glyphs).lines(look);
        facts.extend(self.children(look));
        for part in [
            paint(self.evidence(look)),
            paint(self.decision(look)),
            paint(facts),
            paint(self.next().lines(look)),
        ] {
            if !part.is_empty() {
                out.push('\n');
                out.push_str(&part);
            }
        }
        out
    }

    /// The outcome as a word, and under it what holds the run — the
    /// verdict a reader came for, before anything else.
    fn head(&self, look: &Look) -> Vec<Line> {
        let word = RunWord::of(&self.frame);
        let mut lines = Headline {
            subject: format!("run {}", self.run_id.handle()),
            mark: word.mark(),
            said: word.to_string(),
        }
        .lines(look);
        if let Some((tone, said)) = self.holds() {
            let room = look.width.cells().saturating_sub(INDENT.len());
            lines.extend(
                wrap(&said, room)
                    .into_iter()
                    .map(|part| Line::new().plain(INDENT).push(tone, part)),
            );
        }
        lines
    }

    /// What holds the run where it stopped, when its word leaves that
    /// unsaid.
    fn holds(&self) -> Option<(Tone, String)> {
        let blocking = self.frame.blocking_findings;
        Some(match &self.frame.phase {
            RunPhase::Waiting { on } => (Tone::NeedsYou, advice::parked_in_full(on)),
            RunPhase::Failed {
                failure: Some(failure),
            } => (Tone::Failed, self.failed_claim(failure)),
            RunPhase::Broken { diagnostic } => {
                (Tone::Failed, yunta_core::text::one_line(diagnostic))
            }
            // The work is done and nobody has accepted it.
            RunPhase::Finished if blocking > 0 => (
                Tone::Caution,
                format!("finished holding {}", counted(blocking, "blocking finding")),
            ),
            RunPhase::Promoted { to: Some(mode) } => (Tone::Muted, format!("promoted to `{mode}`")),
            // A run this block is drawn over that has not stopped is
            // reported as what it is doing, not as a stop it has not
            // reached.
            RunPhase::Created | RunPhase::Running => (Tone::Muted, "still moving".to_string()),
            RunPhase::Finished
            | RunPhase::Failed { failure: None }
            | RunPhase::Cancelled
            | RunPhase::Promoted { to: None } => return None,
        })
    }

    /// The failure that closed the run, said as the node it happened to.
    fn failed_claim(&self, failure: &yunta_core::events::Failure) -> String {
        let failed = self.failed().find(|(_, failed)| *failed == failure);
        match failed {
            Some((node, _)) => format!("node `{}` failed: {}", node.id, failure.headline()),
            None => failure.headline(),
        }
    }

    /// Every node that stands failed, with its failure.
    fn failed(&self) -> impl Iterator<Item = (&NodeFrame, &yunta_core::events::Failure)> {
        self.frame
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                NodeStanding::Reached(NodeState::Failed { failure, .. }) => Some((node, failure)),
                _ => None,
            })
    }

    /// What every failed node's failure says beyond its claim: the end
    /// of what a command printed and where the rest is kept, each
    /// document it refused, the paths it should not have touched.
    ///
    /// A node whose failure has nothing more to quote, and whose claim the
    /// line under the outcome already made, is not listed again.
    fn evidence(&self, look: &Look) -> Vec<Line> {
        let held = self.holds().map(|(_, said)| said).unwrap_or_default();
        let mut lines = Vec::new();
        for (node, failure) in self.failed() {
            let whole = failure.output().map(|output| {
                Whole::file(
                    yunta_engine::ObjectStore::at(&self.run_dir).path_of(output),
                    &self.cwd,
                    self.home.as_deref(),
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
                    .push(Tone::Failed, failure.headline()),
            );
            lines.extend(detail.into_iter().map(|line| line.under(INDENT)));
        }
        lines
    }

    /// The decision a parked run waits on, every option with the command
    /// that chooses it. The second line made its claim, and a failed
    /// node's evidence is quoted above it.
    fn decision(&self, look: &Look) -> Vec<Line> {
        let Some((node, escalation)) = &self.decision else {
            return Vec::new();
        };
        let beside = decision::Beside {
            claim: true,
            evidence: self.failed().any(|(failed, _)| failed.id == *node),
        };
        let mut lines = decision::lines(&self.run_id, node, escalation, beside, look);
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
        lines
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
    fn children(&self, look: &Look) -> Vec<Line> {
        if self.frame.children.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![Line::new().plain(INDENT).push(Tone::Muted, "children")];
        for (under, born) in view::children_by_node(&self.frame) {
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

    /// The facts a reader checks once they know how the run stopped.
    fn fields(&self, glyphs: Glyphs) -> Fields {
        let degraded = match self.frame.degraded.len() {
            0 => String::new(),
            n => counted(n, "capability the adapter lacks"),
        };
        Fields::new()
            .push_if(
                "progress",
                crate::render::counter::line(&self.frame, glyphs),
            )
            .push_if("tokens", self.tokens(glyphs))
            .push_if("slowest", self.slowest(glyphs).unwrap_or_default())
            .push_if("branch", self.branch())
            .push_if("artifacts", self.artifacts())
            .push_if("degraded", degraded)
            .push_if(
                "unread",
                unknown_kinds_note(&self.frame.unknown_kinds).unwrap_or_default(),
            )
    }

    /// What a person types next, given how this run stopped, and where
    /// to read the whole of it.
    fn next(&self) -> Next {
        let handle = self.run_id.handle();
        let mut steps = advice::after(RunWord::of(&self.frame), handle, self.decision.is_some());
        steps.push((advice::status(handle), "shows the whole run"));
        Next { steps }
    }

    /// What the run spent, and what this workflow's own past runs spent —
    /// the only honest comparison there is, because it is what already
    /// happened rather than a prediction. Nothing when the run spent
    /// nothing and has no history to compare with.
    fn tokens(&self, glyphs: Glyphs) -> String {
        let spent = self.frame.tokens.total();
        let said = format!("{} spent", Tokens(spent).figure());
        match &self.frame.prior {
            Some(prior) => format!("{said} {} {}", glyphs.sep(), history(prior, glyphs)),
            None if spent == 0 => String::new(),
            None => said,
        }
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
        let tree = paths::shown(&self.worktree, &self.cwd, self.home.as_deref());
        match self.isolation {
            Isolation::Worktree => format!(
                "{} off {}, in {tree}",
                yunta_engine::run_branch(&self.run_id),
                self.base_branch,
            ),
            Isolation::None => format!("{}, in this checkout at {tree}", self.base_branch),
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
                &yunta_engine::run_dir::artifacts_view(&self.run_dir),
                &self.cwd,
                self.home.as_deref()
            )
        )
    }
}

/// What this workflow's past runs cost, in the one phrasing every surface
/// that reports it uses.
fn history(prior: &PriorEstimation, glyphs: Glyphs) -> String {
    crate::commands::stats::format_estimation_line(prior, glyphs)
}

#[cfg(test)]
mod tests;

//! What a run leaves on the terminal once it stops.
//!
//! It opens with the outcome, as a word, because that is the one thing
//! every reader came for; under it goes what the run did, what it cost
//! against what this workflow usually costs, where its work went, and the
//! commands that act on it. A run parked on a person leads with the
//! decision instead: for that reader, the most actionable thing is the
//! most prominent thing.
//!
//! The mark beside the word only repeats it. A run that finished promoted,
//! cancelled, broken, or carrying blocking findings never gets the mark a
//! clean finish gets — a tick over work nobody has accepted is the one
//! decoration that would say something the word does not.

use std::path::{Path, PathBuf};
use std::time::Duration;

use yunta_core::events::{GateWaitingPayload, StoredEvent};
use yunta_core::{Isolation, NodeId, RunId, Workflow};
use yunta_engine::{run_frame, NodeFrame, PriorEstimation, RunFrame, RunPhase};

use crate::commands::status::decision::{self, Layout};
use crate::commands::{advice, counted, unknown_kinds_note};
use crate::error::Outcome;
use crate::render::{format_duration, truncate, Glyphs, StateWord, LABEL_WIDTH};

use super::view;

/// How far a block's body sits under the line that introduces it.
const INDENT: &str = "  ";

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
    blocking: usize,
    run_dir: PathBuf,
    worktree: PathBuf,
    base_branch: String,
    isolation: Isolation,
}

impl Closing {
    /// Derives the block from the run's own log — the truth about what
    /// happened, read once after the run stops rather than carried along
    /// from whatever the surface managed to see.
    pub(crate) fn of(env: ClosingEnv<'_>) -> Self {
        let state = yunta_engine::derive(env.events);
        Self {
            run_id: env.run_id.clone(),
            frame: run_frame(env.run_id, env.workflow, env.events, env.prior, env.now),
            decision: env.decision,
            blocking: yunta_engine::dedup_findings(&state.findings)
                .iter()
                .filter(|finding| finding.severity == yunta_core::events::FindingSeverity::Blocking)
                .count(),
            run_dir: env.outline.run_dir.to_path_buf(),
            worktree: env.outline.worktree.to_path_buf(),
            base_branch: env.outline.base_branch.to_string(),
            isolation: env.outline.isolation,
        }
    }

    /// Whether the invocation reports success. Only a run that finished
    /// does: a paused, failed, cancelled or promoted run ran to a stop
    /// that needs a decision, and its detail is already on this block.
    pub(crate) fn outcome(&self) -> Outcome {
        match self.frame.phase {
            RunPhase::Finished => Outcome::Success,
            _ => Outcome::Reported,
        }
    }

    /// The whole block, ready to print.
    pub(crate) fn render(&self, glyphs: Glyphs) -> String {
        let verdict = self.verdict();
        let mut out = format!(
            "run {}: {} {}\n",
            self.run_id,
            glyphs.state(verdict.word),
            verdict.text
        );
        if let Some((node, escalation)) = &self.decision {
            out.push_str(&decision::block(
                Layout::Trailer,
                &self.run_id,
                node,
                escalation,
            ));
        }
        for (label, value) in self.rows() {
            out.push_str(&format!(
                "{INDENT}{} {value}\n",
                truncate(label, LABEL_WIDTH, glyphs)
            ));
        }
        out
    }

    /// The outcome, as the word a reader acts on and the mark that
    /// repeats it.
    fn verdict(&self) -> Verdict {
        match &self.frame.phase {
            RunPhase::Finished if self.blocking > 0 => Verdict::new(
                StateWord::Wait,
                format!(
                    "finished, holding {}",
                    counted(self.blocking, "blocking finding")
                ),
            ),
            RunPhase::Finished => Verdict::new(StateWord::Done, "finished".to_string()),
            RunPhase::Failed { failure } => Verdict::new(
                StateWord::Fail,
                match failure {
                    Some(failure) => format!("failed — {}", one_line(&failure.to_string())),
                    None => "failed".to_string(),
                },
            ),
            RunPhase::Cancelled => Verdict::new(StateWord::Fail, "cancelled".to_string()),
            RunPhase::Promoted { to } => Verdict::new(
                StateWord::Wait,
                match to {
                    Some(mode) => format!("promoted to `{mode}`"),
                    None => "promoted".to_string(),
                },
            ),
            // A run stopped on a menu says only that here: the block
            // right under this line carries the whole decision, and
            // saying it twice makes a reader check whether the two
            // agree.
            RunPhase::Waiting { .. } if self.decision.is_some() => {
                Verdict::new(StateWord::Wait, "paused on a decision".to_string())
            }
            RunPhase::Waiting { on } => Verdict::new(
                StateWord::Wait,
                format!("paused — {}", advice::parked_on(on)),
            ),
            RunPhase::Broken { diagnostic } => Verdict::new(
                StateWord::Fail,
                format!("broken — {}", one_line(diagnostic)),
            ),
            RunPhase::Created | RunPhase::Running => {
                Verdict::new(StateWord::Run, "still moving".to_string())
            }
        }
    }

    /// The labelled rows under the outcome.
    fn rows(&self) -> Vec<(&'static str, String)> {
        let mut rows = vec![("progress", view::counter_line(&self.frame))];
        rows.push(("tokens", self.tokens()));
        if let Some(slowest) = self.slowest() {
            rows.push(("slowest", slowest));
        }
        rows.push(("branch", self.branch()));
        rows.push(("artifacts", self.artifacts()));
        if !self.frame.degraded.is_empty() {
            rows.push((
                "degraded",
                counted(self.frame.degraded.len(), "capability the adapter lacks"),
            ));
        }
        if let Some(note) = unknown_kinds_note(&self.frame.unknown_kinds) {
            rows.push(("unread", note));
        }
        rows.push(("next", self.next_commands()));
        rows
    }

    /// What the run spent, and what this workflow's own past runs spent —
    /// the only honest comparison there is, because it is what already
    /// happened rather than a prediction.
    fn tokens(&self) -> String {
        let spent = format!("{} spent", self.frame.tokens.total());
        match &self.frame.prior {
            Some(prior) => format!("{spent} · {}", history(prior)),
            None => spent,
        }
    }

    /// The run's longest nodes, longest first. `None` for a run where no
    /// node ever started.
    fn slowest(&self) -> Option<String> {
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
                .map(|(node, elapsed)| format!("{} {}", node.id, format_duration(*elapsed)))
                .collect::<Vec<_>>()
                .join(" · "),
        )
    }

    /// The branch the run worked on and where that work sits now.
    fn branch(&self) -> String {
        match self.isolation {
            Isolation::Worktree => format!(
                "yunta/{} off {}, in {}",
                self.run_id,
                self.base_branch,
                self.worktree.display()
            ),
            Isolation::None => format!(
                "{}, in this checkout at {}",
                self.base_branch,
                self.worktree.display()
            ),
        }
    }

    /// How many artifacts the run's nodes wrote, and the one directory
    /// they are under.
    fn artifacts(&self) -> String {
        let written: usize = self
            .frame
            .nodes
            .iter()
            .map(|node| node.artifacts.len())
            .sum();
        format!(
            "{} under {}",
            counted(written, "artifact"),
            self.run_dir.join("artifacts").display()
        )
    }

    /// What a person does next, given how this run stopped.
    fn next_commands(&self) -> String {
        let status = advice::status(&self.run_id);
        match &self.frame.phase {
            RunPhase::Finished => format!("{} · {status}", advice::receipt(&self.run_id)),
            RunPhase::Waiting { .. } => format!(
                "{} · {status}",
                view::answer_command(&self.run_id, self.decision.is_some())
            ),
            RunPhase::Failed { .. } | RunPhase::Cancelled => {
                format!("{status} · {}", advice::resume(&self.run_id))
            }
            RunPhase::Broken { .. } => format!("{} · {status}", advice::verify(&self.run_id)),
            RunPhase::Created | RunPhase::Running | RunPhase::Promoted { .. } => status,
        }
    }
}

/// The outcome word and the mark that repeats it.
struct Verdict {
    word: StateWord,
    text: String,
}

impl Verdict {
    fn new(word: StateWord, text: String) -> Self {
        Self { word, text }
    }
}

/// What this workflow's past runs cost, in the one phrasing every surface
/// that reports it uses.
fn history(prior: &PriorEstimation) -> String {
    crate::commands::stats::format_estimation_line(prior)
}

/// Prose collapsed onto the one line each row of this block has.
fn one_line(text: &str) -> String {
    yunta_core::text::one_line(text)
}

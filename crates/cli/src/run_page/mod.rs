//! Where a run stands, drawn as one page: what `yunta status` prints for
//! a run that may be moving, and what a run leaves on the terminal once
//! it stops.
//!
//! A page reads in the order a person reads it for: its headline — the
//! workflow, the word the run is called by, how long it has stood there
//! and the handle that names it — the furrow its nodes plow, its gist —
//! what it asks of its reader, or what holds the run — the evidence of
//! what failed, the facts that have something to say, and last the
//! decision it waits on or what to type next. A section with nothing to
//! say is not a section.
//!
//! A page opens at a glance. `--full` tells the whole story — every node with
//! what it said or the plan it accepted, took and spent, every task, the runs
//! it composed, every fact the glance leaves out — and `--dig` adds every trace
//! the run keeps: its whole id, where it lives, the files that hold whole
//! outputs. The closing is the same page in its harvest form: how the run
//! ended, what it took and where its work is. Nothing here paints: the
//! caller draws the document once, with the look of its stream.

mod evidence;
mod facts;
mod story;

pub(crate) use facts::{artifacts_written, pull_requests_of};
pub(crate) use story::handed;

use std::time::Duration;

use chrono::{DateTime, Utc};
use surco::components::{Furrow, Gist, Headline, Next, Note, Prose};
use surco::doc::{Block, Depth, Doc};
use surco::figures::duration;
use surco::look::Look;
use surco::words::{Standing, NOTHING_NEEDS_YOU};
use yunta_core::caution::Caution;
use yunta_core::events::{DecisionOn, Failure, GateWaitingPayload, NodeState, PauseReason};
use yunta_core::shown::ShownDocument;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId};
use yunta_render::command;
use yunta_render::spend::Breakdown;
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame, RunPhase, WaitingOn};
use yunta_replay::RunState;

use crate::commands::advice;
use crate::commands::status::decision::{self, Beside};
use crate::surface::Outline;

/// Which page a run is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// What a run leaves on the terminal once it has stopped.
    Closing,
    /// The page `yunta status` prints for a run.
    Status,
    /// The page `yunta open` writes: a status, drawn where a browser reads it, so it offers
    /// no way to itself.
    Browser,
}

/// What `yunta open` is offered as.
const IN_A_BROWSER: &str = "the whole page, in a browser";

/// The decision a parked run waits on, as the run's log rebuilds it.
#[derive(Debug, Clone)]
pub(crate) struct Decided {
    /// Whom the decision belongs to.
    pub(crate) on: DecisionOn,
    pub(crate) escalation: GateWaitingPayload,
    /// The documents the decision shows, as the run holds them.
    pub(crate) shown: Vec<ShownDocument>,
    /// What the page says beside the decision already.
    pub(crate) beside: Beside,
}

/// What a page is drawn from: one run as its log derives it, the places
/// its page names, the decision it waits on, and how much of it a reader
/// asked for.
pub(crate) struct RunPage<'a> {
    pub(crate) scope: Scope,
    pub(crate) depth: Depth,
    pub(crate) run_id: &'a RunId,
    /// What the run's registry says about the process driving it.
    pub(crate) engine: yunta_replay::registry::Engine,
    pub(crate) frame: &'a RunFrame,
    /// The run as its log derives it; the frame is read from the same log.
    pub(crate) state: &'a RunState,
    /// Where the page's paths are shown from, and the tree the run works in.
    pub(crate) paths: Outline<'a>,
    pub(crate) decision: Option<Decided>,
    /// The change each finished node's accepted plan says it makes, by that
    /// node: the story says it in the node's row, in place of what the node
    /// said last. Empty where the page draws no story.
    pub(crate) plans: Vec<(NodeId, String)>,
}

impl RunPage<'_> {
    /// The page as one document, in the order a person reads it for.
    /// Nothing here paints: `render::draw` does.
    pub(crate) fn document(&self, look: &Look) -> Doc {
        let mut doc = Doc::new().with(self.headline()).with(Furrow(
            self.frame.nodes.iter().map(|node| node.standing).collect(),
        ));
        if let Some(gist) = self.gist() {
            doc = doc.with(Gist::new(gist));
        }
        let doc = doc
            .with_all(self.evidence(look.glyphs))
            .with_all(self.story(look.glyphs))
            .with(self.facts(look.glyphs))
            .with_all(self.changed_files().map(Block::from))
            .with_all(self.children(look.glyphs));
        // What waits on the reader is the last thing on the page, where
        // the eye is when the page ends.
        let decided = self.decided();
        if !decided.is_empty() {
            return doc.with_all(decided);
        }
        doc.with_all(self.no_menu().map(Block::from))
            .with(self.next())
    }

    /// What a run parked on a person says when its pause rebuilds no menu
    /// — a scope, an answers file, a review on a forge, a child: that no
    /// option answers it here, so a reader who knows the menus other stops
    /// show does not look for one.
    fn no_menu(&self) -> Option<Note> {
        let parked = matches!(self.frame.phase, RunPhase::Waiting { .. })
            && self.frame.standing == Standing::NeedsYou;
        (parked && self.decision.is_none()).then(|| {
            Note(
                "no option answers this pause here: settle it where it was raised, then hand \
                 the run back."
                    .to_string(),
            )
        })
    }

    /// The headline: the run's mark, its workflow and mode, its word, how
    /// long it has stood there, and — in a closing — the exit code its
    /// invocation leaves; the handle that names it at the far edge.
    pub(crate) fn headline(&self) -> Headline {
        let word = self.frame.standing;
        let mut headline = Headline::new(
            word.mark(),
            crate::render::called(&self.frame.workflow, &self.frame.mode),
            word.word(),
        );
        for fact in self.stood() {
            headline = headline.fact(fact);
        }
        if self.scope == Scope::Closing {
            let code = crate::render::exit(word).code();
            headline = headline.fact(format!("exit {code}"));
        }
        headline.edge(self.run_id.handle())
    }

    /// How long the run has stood where its word says: for how long an open
    /// run has been there, and how long a closed one took and how long ago
    /// it stopped. Nothing for a log with no event.
    fn stood(&self) -> Vec<String> {
        let last = self.state.clock().last_at();
        let open = matches!(
            self.frame.phase,
            RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
        );
        if self.scope != Scope::Closing && open {
            return self.open_for(last).into_iter().collect();
        }
        let Some(took) = self.frame.elapsed else {
            return Vec::new();
        };
        let mut stood = vec![match self.frame.standing {
            Standing::Finished | Standing::Reported => format!("in {}", duration(took)),
            _ => format!("after {}", duration(took)),
        }];
        if let (true, false, Some(last)) = (self.scope != Scope::Closing, open, last) {
            stood.push(format!("{} ago", duration(self.since(last))));
        }
        stood
    }

    /// How long an open run has stood at its word: since it was asked at
    /// a terminal, since its last event for one that went quiet or parked,
    /// and since it started for one that moves.
    fn open_for(&self, last: Option<DateTime<Utc>>) -> Option<String> {
        if let Some(prompt) = &self.engine.prompt {
            return Some(format!("for {}", duration(self.since(prompt.since))));
        }
        match (self.frame.standing, &self.frame.phase) {
            (Standing::Stalled, _) => {
                last.map(|last| format!("quiet for {}", duration(self.since(last))))
            }
            (_, RunPhase::Waiting { .. }) => {
                last.map(|last| format!("for {}", duration(self.since(last))))
            }
            _ => self
                .frame
                .elapsed
                .map(|took| format!("for {}", duration(took))),
        }
    }

    /// How long ago `then` was, at the instant the page is read at.
    fn since(&self, then: DateTime<Utc>) -> Duration {
        (self.engine.at - then).to_std().unwrap_or_default()
    }

    /// The page's gist: what it asks of its reader, or what holds the run,
    /// as one sentence. `None` when the run's word already says it.
    pub(crate) fn gist(&self) -> Option<String> {
        gist(self.frame, &self.engine)
    }

    /// The decision the run waits on, as the section the page ends on, and
    /// above it where the run works — said whole, it is copied into another
    /// terminal — on a page whose facts do not say it, to a reader who does
    /// not stand there already. A person who has just watched their terminal
    /// stop is the one who needs telling that nothing holds the answer open.
    fn decided(&self) -> Vec<Block> {
        let Some(decided) = self.decision.as_ref() else {
            return Vec::new();
        };
        let mut blocks: Vec<Block> = Vec::new();
        // The closing's branch row says where the run works.
        let shown = surco::paths::shown(self.paths.worktree, self.paths.cwd, self.paths.home);
        if self.scope != Scope::Closing && !self.depth.reaches(Depth::Full) && shown != "." {
            let tree = decision::run_tree_line(self.paths.worktree);
            blocks.push(Prose(tree).into());
        }
        let mut section = decision::section(self.run_id, decided, self.otherwise());
        if self.scope == Scope::Closing {
            section.blocks.push(
                Note(
                    "the run holds its own state on disk, so close this terminal whenever you \
                     like and answer from anywhere."
                        .to_string(),
                )
                .into(),
            );
        }
        blocks.push(section.into());
        blocks
    }

    /// What moves a run that waits on a decision without choosing one of
    /// its options, and, for a person reading `status`, where to read all of it.
    fn otherwise(&self) -> Vec<(String, &'static str)> {
        let mut steps = advice::after(self.frame, self.decided_escalation());
        if self.scope == Scope::Status {
            steps.push((
                command::line(&NextStep::Open {
                    run: self.run_id.clone(),
                }),
                IN_A_BROWSER,
            ));
        }
        steps
    }

    /// What a person types next, for where the run stands, and where to
    /// read more of it.
    fn next(&self) -> Next {
        let mut steps = advice::after(self.frame, self.decided_escalation());
        match (self.scope, self.depth) {
            (Scope::Closing, _) => steps.push((
                command::line(&NextStep::status(self.run_id)),
                "shows the whole run",
            )),
            (Scope::Status, Depth::Glance) => steps.push((
                command::line(&NextStep::Status {
                    run: self.run_id.clone(),
                    node: None,
                    depth: Depth::Full,
                }),
                "the whole story, node by node",
            )),
            (Scope::Status, Depth::Full | Depth::Dig) => {
                steps.push((
                    command::line(&NextStep::Open {
                        run: self.run_id.clone(),
                    }),
                    IN_A_BROWSER,
                ));
            }
            (Scope::Browser, _) => {}
        }
        Next { steps }
    }

    /// The escalation the run stopped on, when the log rebuilds one.
    pub(crate) fn decided_escalation(&self) -> Option<&GateWaitingPayload> {
        self.decision.as_ref().map(|decided| &decided.escalation)
    }

    /// Settled and in-flight spend, split in and out: nothing for a run that
    /// spent none. Every page reads this one figure, from the frame the run's
    /// log derives.
    pub(crate) fn tokens(&self) -> Option<String> {
        let spend = &self.frame.tokens;
        (spend.total() > 0).then(|| Breakdown(spend).to_string())
    }
}

/// The sentence under a page's headline: what the run comes to, as a person reads it. `None`
/// for a run whose word says it all.
pub(crate) fn gist(frame: &RunFrame, engine: &yunta_replay::registry::Engine) -> Option<String> {
    let blocking = frame.blocking_findings;
    Some(match (frame.standing, &frame.phase) {
        (Standing::Stalled, _) => advice::STALLED.to_string(),
        (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => match &engine.prompt {
            Some(prompt) => advice::asking(prompt, None),
            None => format!("it {}", Standing::NeedsYou.word()),
        },
        (_, RunPhase::Waiting { on }) => advice::parked_claim(on),
        (_, RunPhase::Broken { diagnostic }) => surco::text::one_line(diagnostic),
        (
            _,
            RunPhase::Failed {
                failure: Some(failure),
            },
        ) => failed_claim(frame, failure),
        // The work is done and nobody has accepted it.
        (_, RunPhase::Finished) if blocking > 0 => format!(
            "finished holding {}",
            surco::text::counted(blocking, "blocking finding")
        ),
        (_, RunPhase::Promoted { to: Some(mode) }) => format!("promoted to `{mode}`"),
        (_, RunPhase::Created | RunPhase::Running) => {
            format!("{NOTHING_NEEDS_YOU}: it is moving")
        }
        (
            _,
            RunPhase::Finished
            | RunPhase::Failed { failure: None }
            | RunPhase::Cancelled
            | RunPhase::Promoted { to: None },
        ) => return None,
    })
}

/// The failure that closed the run, said as the node it happened to.
fn failed_claim(frame: &RunFrame, failure: &Failure) -> String {
    let headline = yunta_render::failure::headline(failure);
    match failed(frame).find(|(_, found)| *found == failure) {
        Some((node, _)) => format!("node `{}` failed: {headline}", node.id),
        None => headline,
    }
}

/// The nodes that stand failed, each with its failure, in declaration order.
/// A node whose log says it failed can stand otherwise: one a reset holds is
/// `waiting`, and the pause says what holds it; one a reroute corrects is
/// `rerouted`. Neither is listed.
pub(crate) fn failed(frame: &RunFrame) -> impl Iterator<Item = (&NodeFrame, &Failure)> {
    frame.nodes.iter().filter_map(|node| match &node.state {
        NodeStanding::Reached(NodeState::Failed { failure, .. })
            if node.standing == Standing::Failed =>
        {
            Some((node, failure))
        }
        _ => None,
    })
}

/// The cautions a page says beside its facts. A run parked on its token
/// budget says what the declared cap means against what this workflow's
/// past runs spent; a run parked on anything else says none of it.
pub(crate) fn cautions(frame: &RunFrame) -> Vec<Caution> {
    match &frame.phase {
        RunPhase::Waiting {
            on:
                WaitingOn::Run {
                    reason: PauseReason::BudgetExhausted { cap, .. },
                },
        } => yunta_replay::history::budget_caution(Some(*cap), frame.prior.as_ref())
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// The cautions of a run, each in the words render gives it, as one row.
pub(crate) fn caution_row(frame: &RunFrame) -> String {
    cautions(frame)
        .iter()
        .map(yunta_render::caution::line)
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod tests;

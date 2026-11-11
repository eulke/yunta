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

mod browser;
mod evidence;
mod facts;
mod story;

pub use browser::RunInABrowser;

use std::path::PathBuf;

use super::Page;
use crate::command::{self, Reader};
use crate::escalation::{self, Beside, Decision};
use crate::spend::Breakdown;
use surco::components::{Furrow, Gist, Headline, Next, Note, Prose};
use surco::doc::{Block, Depth, Doc};
use surco::figures::duration;
use surco::look::Look;
use surco::words::Standing;
use yunta_core::events::GateWaitingPayload;
use yunta_core::step::NextStep;
use yunta_core::{Isolation, NodeId, RunId};
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::RunState;

/// Where a run lives and where its paths are shown from, as the caller found
/// them: what a page names that the run's own log does not carry.
#[derive(Debug, Clone)]
pub struct Places {
    pub run_dir: PathBuf,
    pub worktree: PathBuf,
    /// The run's own branch.
    pub branch: String,
    /// The branch it started from, as its manifest froze it.
    pub base_branch: String,
    pub isolation: Isolation,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
}

/// Which page a run is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunScope {
    /// What a run leaves on the terminal once it has stopped, with the code
    /// the process that drove it exits with.
    Closing { exit_code: u8 },
    /// The page `yunta status` prints for a run.
    Status,
    /// The page `yunta open` writes: a status, drawn where a browser reads it, so it offers
    /// no way to itself.
    Browser,
}

/// What `yunta open` is offered as.
const IN_A_BROWSER: &str = "the whole page, in a browser";

/// What a page is drawn from: one run as its log derives it, the places
/// its page names, the decision it waits on, and how much of it a reader
/// asked for.
#[derive(Debug, Clone)]
pub struct Run {
    pub scope: RunScope,
    pub depth: Depth,
    pub run_id: RunId,
    /// The run at the instant the page is read at: what drives it, and how
    /// long it has stood where it is, measured there.
    pub frame: RunFrame,
    /// The run as its log derives it; the frame is read from the same log.
    pub state: RunState,
    /// Where the page's paths are shown from, and the tree the run works in.
    pub places: Places,
    pub decision: Option<Decision>,
    /// The change each finished node's accepted plan says it makes, by that
    /// node: the story says it in the node's row, in place of what the node
    /// said last. Empty where the page draws no story.
    pub plans: Vec<(NodeId, String)>,
}

impl Page for Run {
    /// The page as one document, in the order a person reads it for.
    /// Nothing here paints: the surface it is drawn on does.
    fn document(&self, look: &Look) -> Doc {
        let mut doc = Doc::new().with(self.headline()).with(Furrow(
            self.frame.nodes.iter().map(|node| node.standing).collect(),
        ));
        if let Some(gist) = crate::state::gist(&self.frame) {
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
}

impl Run {
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
    pub fn headline(&self) -> Headline {
        let word = self.frame.standing;
        let mut headline = Headline::new(
            word.mark(),
            crate::state::called(&self.frame.workflow, &self.frame.mode),
            word.word(),
        );
        for fact in self.stood() {
            headline = headline.fact(fact);
        }
        if let RunScope::Closing { exit_code } = self.scope {
            headline = headline.fact(format!("exit {exit_code}"));
        }
        headline.edge(self.run_id.handle())
    }

    /// How long the run has stood where its word says: for how long an open
    /// run has been there, and how long a closed one took and how long ago
    /// it stopped. Nothing for a log with no event.
    fn stood(&self) -> Vec<String> {
        let open = matches!(
            self.frame.phase,
            RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
        );
        if !matches!(self.scope, RunScope::Closing { .. }) && open {
            return self.open_for().into_iter().collect();
        }
        let Some(took) = self.frame.elapsed else {
            return Vec::new();
        };
        let mut stood = vec![match self.frame.standing {
            Standing::Finished | Standing::Reported => format!("in {}", duration(took)),
            _ => format!("after {}", duration(took)),
        }];
        if let (true, false, Some(ago)) = (
            !matches!(self.scope, RunScope::Closing { .. }),
            open,
            self.frame.since_last_event,
        ) {
            stood.push(format!("{} ago", duration(ago)));
        }
        stood
    }

    /// How long an open run has stood at its word: since it was asked at
    /// a terminal, since its last event for one that went quiet or parked,
    /// and since it started for one that moves.
    fn open_for(&self) -> Option<String> {
        if let Some(asking) = &self.frame.asking {
            return Some(format!("for {}", duration(asking.waited)));
        }
        let quiet = self.frame.since_last_event;
        match (self.frame.standing, &self.frame.phase) {
            (Standing::Stalled, _) => quiet.map(|quiet| format!("quiet for {}", duration(quiet))),
            (_, RunPhase::Waiting { .. }) => quiet.map(|stood| format!("for {}", duration(stood))),
            _ => self
                .frame
                .elapsed
                .map(|took| format!("for {}", duration(took))),
        }
    }

    /// The decision the run waits on, as the section the page ends on, and
    /// above it where the run works — said whole, it is copied into another
    /// terminal — on a page whose facts do not say it, to a reader who does
    /// not stand there already. A person who has just watched their terminal
    /// stop is the one who needs telling that nothing holds the answer open.
    fn decided(&self) -> Vec<Block> {
        let Some(decision) = &self.decision else {
            return Vec::new();
        };
        let mut blocks: Vec<Block> = Vec::new();
        // The closing's branch row says where the run works.
        let shown = surco::paths::shown(
            &self.places.worktree,
            &self.places.cwd,
            self.places.home.as_deref(),
        );
        if !matches!(self.scope, RunScope::Closing { .. })
            && !self.depth.reaches(Depth::Full)
            && shown != "."
        {
            let tree = escalation::run_tree_line(&self.places.worktree);
            blocks.push(Prose(tree).into());
        }
        // A failed node's evidence is quoted above the decision, so the
        // decision does not repeat it.
        let beside = Beside {
            evidence: self
                .frame
                .failed()
                .any(|(node, _)| decision.on.node() == Some(&node.id)),
        };
        let mut section = decision.section(&self.run_id, beside, self.otherwise());
        if matches!(self.scope, RunScope::Closing { .. }) {
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

    /// What the run offers a person next, each action as its line and what
    /// it does.
    fn offered(&self) -> Vec<(String, &'static str)> {
        self.frame
            .actions(self.decided_escalation())
            .iter()
            .map(|action| command::said(action, Reader::Person))
            .collect()
    }

    /// What moves a run that waits on a decision without choosing one of
    /// its options, and, for a person reading `status`, where to read all of it.
    fn otherwise(&self) -> Vec<(String, &'static str)> {
        let mut steps = self.offered();
        if self.scope == RunScope::Status {
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
        let mut steps = self.offered();
        match (self.scope, self.depth) {
            (RunScope::Closing { .. }, _) => steps.push((
                command::line(&NextStep::status(&self.run_id)),
                "shows the whole run",
            )),
            (RunScope::Status, Depth::Glance) => steps.push((
                command::line(&NextStep::Status {
                    run: self.run_id.clone(),
                    node: None,
                    depth: Depth::Full,
                }),
                "the whole story, node by node",
            )),
            (RunScope::Status, Depth::Full | Depth::Dig) => {
                steps.push((
                    command::line(&NextStep::Open {
                        run: self.run_id.clone(),
                    }),
                    IN_A_BROWSER,
                ));
            }
            (RunScope::Browser, _) => {}
        }
        Next { steps }
    }

    /// The escalation the run stopped on, when the log rebuilds one.
    pub(crate) fn decided_escalation(&self) -> Option<&GateWaitingPayload> {
        self.decision.as_ref().map(|decision| &decision.escalation)
    }

    /// Settled and in-flight spend, split in and out: nothing for a run that
    /// spent none. Every page reads this one figure, from the frame the run's
    /// log derives.
    pub(crate) fn tokens(&self) -> Option<String> {
        let spend = &self.frame.tokens;
        (spend.total() > 0).then(|| Breakdown(spend).to_string())
    }
}

/// The cautions of a run, each in the words render gives it, as one row.
pub(crate) fn caution_row(frame: &RunFrame) -> String {
    frame
        .cautions()
        .iter()
        .map(crate::caution::line)
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod tests;

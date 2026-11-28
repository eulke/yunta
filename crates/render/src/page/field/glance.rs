//! See [`super`]. The run the field points at, at a glance beside the field's list: what a
//! person reads of it before they open it, and what they can do about it from the field.
//!
//! Its headline names the run's workflow and mode and its word, the handle at the far edge
//! where the room leaves it, since the pointed row names the run too; its furrow is drawn
//! apart, a stroke a node; its gist says what holds it, as its row says it where its engine
//! asks at its terminal and as its page says it otherwise. Its facts are the run's page's, each
//! worded once there: how far it has come, what it spent, where its time went, its slowest
//! nodes and its branch, for a run at work or ended; how long it has waited and its branch,
//! for one that waits. Its acts are the keys the field takes for them, each said as
//! [`GlanceAct::said`] says it. A run the field cannot derive is named, with why, and offers
//! nothing.
//!
//! The clock's part is the row's (`FieldRow::times`), worked out at the instant the field is
//! drawn: nothing here keeps a time.

use surco::components::{AtAGlance, Binding, Furrow, Gist, Headline, Keys};
use surco::doc::{Block, Doc};
use surco::figures::age;
use surco::look::Look;
use surco::words::Standing;
use yunta_core::{Isolation, RunId};
use yunta_replay::frame::Effect;

use super::{FieldGroup, FieldRow, UnreadableRun, UNREADABLE};
use crate::page::run::facts;
use crate::page::Page;
use crate::{command, engine, progress, screen, state};

/// What `enter` does on the glance at a run that waits on a decision: opens the run where the
/// decision is read.
const READ_THE_DECISION: &str = "read the decision";

/// What the glance calls how long a run has waited, beside it.
const WAITING: &str = "waiting";

/// The run the field points at, at a glance: its headline, its furrow apart, its gist, its
/// facts and what a person can do about it from the field, a key a row; or a run the field
/// names but cannot derive, with why. Not [`Run`](crate::page::Run), the page `yunta status`
/// prints.
pub enum RunAtAGlance<'a> {
    /// A run its log derives, as the field reads it.
    Read {
        row: &'a FieldRow,
        /// The branch the run started from, as its manifest froze it.
        base_branch: &'a str,
        /// Whether the run works in a checkout of its own, on a branch of its own.
        isolation: Isolation,
        /// What the field offers on it, each with the key that takes it there.
        acts: &'a [(String, GlanceAct)],
    },
    /// A run the field names but cannot derive.
    Unread(&'a UnreadableRun),
}

/// What an act the glance offers does to the run, which the glance says beside its key: an act
/// the run offers, by what it does to the run, or one of the glance's own, which open the run
/// where its decision or its question is read, or open its numbers. Not an `Action`, a step
/// with what it does, whose step the field's key stands for here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlanceAct {
    /// An act `RunFrame::actions` offers, by its effect.
    Offered(Effect),
    /// Opening the run where the decision it waits on is read.
    ReadsTheDecision,
    /// Opening the run where what it asks is answered.
    AnswersWhatItAsks,
    /// Opening what the run cost.
    ShowsWhatItCost,
}

impl GlanceAct {
    /// What it does, as the glance says it beside its key: an act the run offers in the
    /// palette's words, as `yunta status` says it, but handing a run back, which the glance
    /// says as a run's legend does; the glance's own keys in words of their own.
    pub fn said(self) -> &'static str {
        match self {
            GlanceAct::Offered(Effect::HandsBackOnceSettled) => {
                screen::in_the_legend(Effect::HandsBackOnceSettled)
            }
            GlanceAct::Offered(effect) => command::effect(effect),
            GlanceAct::ReadsTheDecision => READ_THE_DECISION,
            GlanceAct::AnswersWhatItAsks => command::ANSWER_WHAT_IT_ASKS,
            GlanceAct::ShowsWhatItCost => screen::WHAT_IT_COST,
        }
    }
}

impl Page for RunAtAGlance<'_> {
    fn document(&self, _look: &Look) -> Doc {
        match self {
            RunAtAGlance::Read {
                row,
                base_branch,
                isolation,
                ..
            } => {
                let frame = &row.frame;
                let headline = Headline::new(
                    frame.standing.mark(),
                    state::called(&frame.workflow, &frame.mode),
                    frame.standing.word(),
                )
                .edge_said_elsewhere(frame.run_id.handle());
                let furrow = Furrow(frame.nodes.iter().map(|node| node.standing).collect());
                let glance = AtAGlance {
                    facts: read_facts(row, base_branch, *isolation),
                    acts: self.acts(),
                };
                Doc::new()
                    .with(headline)
                    .with(furrow)
                    .with_all(gist(row).map(|gist| Block::from(Gist::new(gist))))
                    .with(glance)
            }
            RunAtAGlance::Unread(run) => Doc::new()
                .with(
                    Headline::new(Standing::Broken.mark(), UNREADABLE, "")
                        .edge_said_elsewhere(run.run_id.handle()),
                )
                .with(Gist::new(run.problem.to_string())),
        }
    }
}

impl RunAtAGlance<'_> {
    /// The acts alone, under their heading: what the field says of the run pointed at under its
    /// list where the glance gives way. None where the run offers nothing from the field.
    pub fn what_you_can_do(&self) -> Option<Block> {
        let acts = self.acts()?;
        Some(
            AtAGlance {
                facts: Vec::new(),
                acts: Some(acts),
            }
            .into(),
        )
    }

    /// What the glance offers, under its heading, a key a row; none where it offers nothing.
    fn acts(&self) -> Option<(String, Vec<Binding>)> {
        let RunAtAGlance::Read { acts, .. } = self else {
            return None;
        };
        let bindings: Vec<Binding> = acts
            .iter()
            .map(|(key, act)| Binding {
                keys: Keys::Typed(key.clone()),
                does: act.said().to_string(),
            })
            .collect();
        (!bindings.is_empty()).then(|| (screen::WHAT_YOU_CAN_DO.to_string(), bindings))
    }
}

/// What holds the run, as the glance's gist says it: whom its engine asks at its terminal, as
/// its row says it, or its page's gist.
fn gist(row: &FieldRow) -> Option<String> {
    match &row.times.asking {
        Some(asking) if row.asking() => Some(engine::asking_on(asking)),
        _ => state::gist(&row.frame, &row.times),
    }
}

/// What the glance says of the run of `row`, each fact beside its label, in the run's page's
/// words: for a run that waits, how long and its branch; for one at work or ended, how far it
/// came, what it spent, where its time went, its slowest nodes and its branch.
fn read_facts(
    row: &FieldRow,
    base_branch: &str,
    isolation: Isolation,
) -> Vec<(&'static str, String)> {
    let branch = branch(&row.frame.run_id, base_branch, isolation);
    let waits = match row.group() {
        FieldGroup::NeedsYou => !row.asking(),
        FieldGroup::Stalled | FieldGroup::Stopped => true,
        FieldGroup::Running | FieldGroup::Closed => false,
    };
    if waits {
        return vec![
            (WAITING, facts::stood_for(age(row.stood()))),
            (facts::BRANCH, branch),
        ];
    }
    let frame = &row.frame;
    vec![
        (facts::PROGRESS, progress::row(frame)),
        (facts::TOKENS, facts::tokens(frame).unwrap_or_default()),
        (
            facts::TIME,
            facts::time_row(&row.times.time).unwrap_or_default(),
        ),
        (
            facts::SLOWEST,
            facts::slowest(frame, &row.times).unwrap_or_default(),
        ),
        (facts::BRANCH, branch),
    ]
}

/// The run's branch and the one it started from, as the glance says them: its own branch
/// named by the run's handle, short enough for the glance, where it works on one; the branch
/// it works on otherwise.
fn branch(run_id: &RunId, base_branch: &str, isolation: Isolation) -> String {
    match isolation {
        // Three dots in every glyph set, as the design's short form of a run's branch writes
        // them: the handle names the run, and the whole branch is on its page.
        Isolation::Worktree => facts::off(
            &format!("{}/...{}", RunId::BRANCHES, run_id.handle()),
            base_branch,
        ),
        Isolation::None => base_branch.to_string(),
    }
}

#[cfg(test)]
mod tests;

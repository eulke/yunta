//! What `yunta resolve-gate` answers with, as a person reads it: the decision
//! it recorded, or that nothing was and the run still waits; the one sentence
//! either door says a recorded decision in; and the same for the questions a
//! parked node asked, answered as a round.

use surco::components::{Gist, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::words::Standing;
use yunta_core::step::{Choice, NextStep};
use yunta_core::{NodeId, OptionId, RunId};

use super::Page;
use crate::command;

/// How a gate's answer ended.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Resolution {
    /// The decision is recorded and the run is handed back.
    Resolved { option: OptionId },
    /// Nothing was recorded: the person at the menu chose none, and the run
    /// keeps waiting on its decision.
    Declined,
    /// Nothing was recorded: the menu was interrupted, and the exit code says so.
    Interrupted,
}

/// A gate's answer, as the person who gave it reads it: what was recorded,
/// or that nothing was and the run still waits.
pub struct GateAnswered<'a> {
    pub run_id: &'a RunId,
    pub resolution: &'a Resolution,
}

impl Page for GateAnswered<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let handle = self.run_id.handle();
        match self.resolution {
            Resolution::Resolved { option } => resolved(self.run_id, option),
            Resolution::Declined => Doc::new()
                .with(Headline::new(
                    Standing::NeedsYou.mark(),
                    format!("run {handle}"),
                    Standing::NeedsYou.word(),
                ))
                .with(Gist::new(
                    "nothing recorded: it still waits on the decision",
                ))
                .with(Next {
                    steps: vec![(
                        command::line(&NextStep::ResolveGate {
                            run: self.run_id.clone(),
                            choice: Choice::Asked,
                        }),
                        "puts the decision to you again",
                    )],
                }),
            Resolution::Interrupted => Doc::new(),
        }
    }
}

impl GateAnswered<'_> {
    /// The sentence a recorded decision is answered with, at either door:
    /// the command draws the page, and the control plane's `resolve_gate`
    /// tool returns this.
    pub fn sentence(run_id: &RunId, option: &OptionId) -> String {
        format!("run {run_id}: {}; {APPLIED}", resolved_as(option))
    }
}

/// How answering the questions a run parked on ended.
#[derive(Debug, Clone)]
pub enum RoundAnswer {
    /// The answers are recorded, and the run is handed back.
    Answered { node: NodeId },
    /// Nothing was recorded: the person left the round, and the run keeps waiting
    /// on the questions `node` asked.
    Declined { node: NodeId },
    /// Nothing was recorded: the round was interrupted, and the exit code says so.
    Interrupted,
}

/// A parked round's answer, as the person who gave it reads it: what was recorded,
/// or that nothing was and the run still waits.
pub struct QuestionsAnswered<'a> {
    pub run_id: &'a RunId,
    pub answer: &'a RoundAnswer,
}

impl Page for QuestionsAnswered<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let handle = self.run_id.handle();
        let again = |does: &'static str| Next {
            steps: vec![(
                command::line(&NextStep::ResolveGate {
                    run: self.run_id.clone(),
                    choice: Choice::Asked,
                }),
                does,
            )],
        };
        match self.answer {
            RoundAnswer::Answered { node } => Doc::new()
                .with(Headline::new(
                    Mark::Done,
                    format!("run {handle}"),
                    format!("answered node `{node}`"),
                ))
                .with(Gist::new(CARRIED_ON))
                .with(Next {
                    steps: vec![(
                        command::line(&NextStep::Follow {
                            run: self.run_id.clone(),
                        }),
                        "follows it",
                    )],
                }),
            RoundAnswer::Declined { node } => Doc::new()
                .with(Headline::new(
                    Standing::NeedsYou.mark(),
                    format!("run {handle}"),
                    Standing::NeedsYou.word(),
                ))
                .with(Gist::new(format!(
                    "nothing recorded: it still waits on the questions node `{node}` asked"
                )))
                .with(again("puts the questions to you again")),
            RoundAnswer::Interrupted => Doc::new(),
        }
    }
}

impl QuestionsAnswered<'_> {
    /// The sentence answered questions are said in to a client of the control
    /// plane, whose `answer_questions` tool returns it.
    pub fn sentence(run_id: &RunId, node: &NodeId) -> String {
        format!(
            "run {run_id}: node `{node}` answered, {}",
            crate::start::INDEPENDENTLY
        )
    }

    /// What `resolve-gate` says off a terminal of a run parked on questions, where
    /// nobody can be asked them: the node that asked, and where they are asked.
    pub fn off_terminal(run_id: &RunId, node: &NodeId) -> String {
        format!(
            "run {} waits on the questions node `{node}` asked, and no terminal is here to ask \
             them; `{}` asks them at one",
            run_id.handle(),
            command::line(&NextStep::ResolveGate {
                run: run_id.clone(),
                choice: Choice::Asked,
            })
        )
    }
}

/// What a resume does with answered questions: it carries the node on with them.
const CARRIED_ON: &str = "a resume in the background carries the run on with them";

/// What a resume does with a recorded decision, in the words both doors
/// say: it applies it, and the menu it was chosen from says what that does.
const APPLIED: &str = "a resume in the background applies it";

/// What a recorded decision is said to be, at either door.
fn resolved_as(option: &OptionId) -> String {
    format!("resolved `{option}`")
}

/// What a recorded decision is answered with: the option, and that a
/// resume in the background applies it.
fn resolved(run_id: &RunId, option: &OptionId) -> Doc {
    let handle = run_id.handle();
    Doc::new()
        .with(Headline::new(
            Mark::Done,
            format!("run {handle}"),
            resolved_as(option),
        ))
        .with(Gist::new(APPLIED))
        .with(Next {
            steps: vec![(
                command::line(&NextStep::Follow {
                    run: run_id.clone(),
                }),
                "follows it",
            )],
        })
}

#[cfg(test)]
mod tests;

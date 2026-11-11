//! See [`super`]. What a run waits on, put to the person: the round of questions it is
//! parked on, or the decision its gate waits on, as `resolve-gate` puts them at a
//! terminal, the round first.
//!
//! A decision stands as its card in the main region with the documents it stands under
//! beside it, or as the desk of the plan it is on, taking the screen while the plan is
//! in front. It is `prompt`'s [`Deciding`], put from the decision the run's log
//! rebuilds, the one the CLI holds, so its keys, its words and its refusals are the
//! same. A round is `prompt`'s [`Round`], the one the run itself asks: nothing is
//! recorded until its last answer.
//!
//! What is chosen or answered is recorded and the run handed on to a detached resume,
//! and the person goes back to where they came from; leaving goes back without
//! recording anything.

use surco::components::{Note, PaneLaid, Region, Screen, Side};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::{NodeId, RunId};
use yunta_render::page::{Gate, Page};
use yunta_render::prompt::Escape;
use yunta_render::refusal::Refused;

use super::Want;
use crate::prompt::{Answerable, Deciding, DecisionPut, Opens, Press, Round, Stroke};
use crate::{Operation, Outcome};

/// What a run waits on, being answered.
#[derive(Debug, Clone)]
pub struct DecisionStage {
    run: RunId,
    /// What the decision opens on: its card, or the plan it is on.
    opens: Opens,
    put: Put,
    /// What the last piece of work came to where it did not do what was asked.
    said: Option<String>,
}

/// What the stage puts to the person.
#[derive(Debug, Clone)]
enum Put {
    /// The round the run is parked on, being read.
    ReadingTheRound,
    /// The decision the run waits on, being read.
    ReadingTheDecision,
    /// A decision, with its card and the plan behind it as the person stands on them.
    Decision(Box<Deciding>),
    /// The round of questions `node` asked, as far as it is answered.
    Round { node: NodeId, round: Box<Round> },
}

impl DecisionStage {
    /// What `run` waits on, before it is read: the round it is parked on, or else its
    /// decision, opening on its card; or, for a person who came to read the plan
    /// (`Opens::ThePlan`), its decision alone.
    pub(crate) fn of(run: RunId, opens: Opens) -> Self {
        let put = match opens {
            Opens::TheCard => Put::ReadingTheRound,
            Opens::ThePlan => Put::ReadingTheDecision,
        };
        DecisionStage {
            run,
            opens,
            put,
            said: None,
        }
    }

    /// What the stage asks for as it opens: what it is to read first.
    pub(crate) fn opening(&self) -> Vec<Want> {
        let run = self.run.clone();
        let reading = match self.put {
            Put::ReadingTheRound => Operation::ReadQuestions { run },
            _ => Operation::ReadDecision { run },
        };
        vec![Want::Work(reading)]
    }

    /// What the stage puts, as a screen: a round, or the one of a decision's card and
    /// plan in front, beside the card the documents it stands under. The prompts carry
    /// their keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = Doc::new();
        let mut side = None;
        match &self.put {
            Put::Decision(deciding) => {
                doc = doc.with(deciding.block(look));
                if !deciding.plan_in_front() {
                    side = beside_the_card(deciding.decision(), look);
                }
            }
            Put::Round { round, .. } => doc = doc.with(round.block(look)),
            Put::ReadingTheRound | Put::ReadingTheDecision => {}
        }
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen = Screen::new(Region { title: None, doc });
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            self.run.handle().to_string(),
            yunta_render::screen::DECISION.to_string(),
        ];
        screen.side = side;
        screen
    }

    /// What `stroke` asks for here: what the prompt in front makes of it, what is chosen
    /// or answered recorded; leaving goes back, and Ctrl-C leaves.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        self.said = None;
        let run = self.run.clone();
        match &mut self.put {
            Put::Decision(deciding) => wanted(deciding.press(stroke), |picked| Operation::Decide {
                run,
                option: picked.option,
                words: picked.words,
            }),
            Put::Round { node, round } => {
                let node = node.clone();
                wanted(round.press(stroke), |settled| Operation::Answer {
                    run,
                    node,
                    answers: settled.answers,
                })
            }
            Put::ReadingTheRound | Put::ReadingTheDecision => match stroke {
                Stroke::Decline => vec![Want::Back],
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
        }
    }

    /// Whether words are being typed: for an option of a decision, or for a question.
    pub(crate) fn typing(&self) -> bool {
        match &self.put {
            Put::Decision(deciding) => deciding.typing(),
            Put::Round { round, .. } => round.typing(),
            Put::ReadingTheRound | Put::ReadingTheDecision => false,
        }
    }

    /// What a surface laid out of the desk's pane, which the desk reads on the next
    /// stroke.
    pub(crate) fn laid(&mut self, pane: PaneLaid) {
        match &mut self.put {
            Put::Decision(deciding) => deciding.laid(pane),
            Put::Round { round, .. } => round.laid(pane),
            Put::ReadingTheRound | Put::ReadingTheDecision => {}
        }
    }

    /// What an answer to work the stage asked for does: what was read is put to the
    /// person, a run parked on no round read for its decision instead, and a round of no
    /// questions answered with nothing; once something is recorded, the person goes
    /// back to where they came from; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        let run = self.run.clone();
        match outcome {
            Outcome::Questions(waiting) if waiting.questions.questions.is_empty() => {
                vec![Want::Work(Operation::Answer {
                    run,
                    node: waiting.node,
                    answers: Vec::new(),
                })]
            }
            Outcome::Questions(waiting) => {
                let round = Round::new(&waiting.node, &waiting.questions, Escape::LeavesWaiting);
                self.put = Put::Round {
                    node: waiting.node,
                    round: Box::new(round),
                };
                Vec::new()
            }
            Outcome::Refused(Refused::NoDecision { .. })
                if matches!(self.put, Put::ReadingTheRound) =>
            {
                self.put = Put::ReadingTheDecision;
                vec![Want::Work(Operation::ReadDecision { run })]
            }
            Outcome::Decision(waiting) => {
                let put = DecisionPut::waiting(&run, &waiting);
                let deciding = Deciding::put(&put, self.opens, Escape::LeavesWaiting);
                self.put = Put::Decision(Box::new(deciding));
                Vec::new()
            }
            Outcome::Decided { .. } | Outcome::Answered { .. } => vec![Want::Back],
            Outcome::Refused(refused) => {
                self.said = Some(refused.to_string());
                Vec::new()
            }
            Outcome::Failed(said) => {
                self.said = Some(said);
                Vec::new()
            }
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Started { .. }
            | Outcome::Field { .. }
            | Outcome::Followed
            | Outcome::Page(_)
            | Outcome::NodePage(_)
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Stopped => Vec::new(),
        }
    }
}

/// What a decision's card stands under, beside it: the documents the desk does not
/// show, and what weighs on it; nothing where there is neither.
fn beside_the_card(put: &DecisionPut, look: &Look) -> Option<Side> {
    let above = Gate::AboveTheCard {
        shown: put.run().map(|run| (run, put.documents())),
        escalation: &put.escalation,
    }
    .document(look);
    (!above.blocks().is_empty()).then_some(Side {
        region: Region {
            title: None,
            doc: above,
        },
        focused: false,
    })
}

/// What a prompt's `press` asks of the stage: its answer recorded by the work `recorded`
/// makes of it, going back for a prompt left, and leaving for Ctrl-C.
fn wanted<T>(press: Press<T>, recorded: impl FnOnce(T) -> Operation) -> Vec<Want> {
    match press {
        Press::Answer(answer) => vec![Want::Work(recorded(answer))],
        Press::Decline => vec![Want::Back],
        Press::Interrupt => vec![Want::Leave],
        Press::Stay | Press::Moved => Vec::new(),
    }
}

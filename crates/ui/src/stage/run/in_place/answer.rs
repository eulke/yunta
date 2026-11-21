//! See [`super`]. How an answer given on a prompt in place is asked for and where it stands:
//! a stroke of the prompt's own turns its decision or its round, the plan read from the card
//! and the card brought back from it, an option chosen on the card or confirmed at the plan's
//! desk, or a round's every answer once its last is given, asked to be recorded on the run's
//! log through the work the performer runs, once; the prompt answers no key while it is under
//! way, and takes its keys again where it is handed back refused or failed.

use yunta_core::{NodeId, RunId};

use super::{InPlace, Posed};
use crate::prompt::{Answerable, Deciding, PickedOption, Press, Round, Stroke};
use crate::stage::Want;
use crate::Operation;

/// Where the answer given on a prompt in place stands. Not the run's log, which says what
/// was recorded: what the screen waits on of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Answer {
    /// None given yet: the prompt answers its keys.
    Open,
    /// An answer asked to be recorded, and what the run does once it is, as the note will
    /// say it.
    UnderWay(String),
    /// Recorded on the run's log: the pause it belongs to is going.
    Recorded,
}

impl InPlace {
    /// What `stroke`, one of its own, asks for on the run `run`: an option chosen on the card
    /// or confirmed at the desk, or a round's every answer once its last is given, recorded on
    /// the run's log; Ctrl-C leaving the application; nothing while an answer is under way or
    /// recorded, nor where the stroke turns the prompt, to the plan the card offers among it.
    pub(super) fn pressed(&mut self, stroke: Stroke, run: &RunId) -> Vec<Want> {
        if self.answer != Answer::Open {
            return Vec::new();
        }
        let (wants, answer) = match &mut self.posed {
            Posed::Decision(deciding) => chosen(deciding, stroke, run),
            Posed::Round { node, round } => answered((node, round), stroke, run),
        };
        self.answer = answer;
        wants
    }

    /// What the run does now that its log recorded the answer under way, for the note: none
    /// where no answer was under way here.
    pub(super) fn recorded(&mut self) -> Option<String> {
        let Answer::UnderWay(said) = &self.answer else {
            return None;
        };
        let said = said.clone();
        self.answer = Answer::Recorded;
        Some(said)
    }

    /// The answer under way handed back, refused or failed: the prompt answers its keys
    /// again.
    pub(super) fn unrecorded(&mut self) {
        if let Answer::UnderWay(_) = self.answer {
            self.answer = Answer::Open;
        }
    }
}

/// What `stroke` asks for on the decision `deciding`, which the run `run` waits on, at its
/// card or at the plan's desk, whichever stands in front, as the decision turns by it, and
/// where the answer stands after it: an option chosen on the card or confirmed at the desk
/// asked to be recorded on the run's log, under way with what the run does once it is; Ctrl-C
/// typed in a field leaving the application; nothing where it turns the decision, the plan
/// the card offers in front, the card back from the plan, a number named, a line opened.
fn chosen(deciding: &mut Deciding, stroke: Stroke, run: &RunId) -> (Vec<Want>, Answer) {
    match deciding.press(stroke) {
        Press::Answer(picked) => decided(deciding, picked, run),
        Press::Interrupt => (vec![Want::Leave], Answer::Open),
        Press::Stay | Press::Moved | Press::Decline => (Vec::new(), Answer::Open),
    }
}

/// `picked`, chosen on the decision `deciding` the run `run` waits on, asked to be recorded
/// on the run's log, and the answer under way with what the run does once it is.
fn decided(deciding: &Deciding, picked: PickedOption, run: &RunId) -> (Vec<Want>, Answer) {
    let put = deciding.decision();
    let words = picked.words.as_deref();
    let said = yunta_render::screen::recorded(&put.escalation, &picked.option, words, &put.ahead);
    let decide = Operation::Decide {
        run: run.clone(),
        option: picked.option,
        words: picked.words,
    };
    (vec![Want::Work(decide)], Answer::UnderWay(said))
}

/// What `stroke` asks for on the round `round` the node `node` of the run `run` asks, and
/// where the answer stands after it: once its last question is answered, every answer asked
/// to be recorded on the run's log at once, under way with how many and what the run does
/// with them; Ctrl-C leaving the application.
fn answered(
    (node, round): (&NodeId, &mut Round),
    stroke: Stroke,
    run: &RunId,
) -> (Vec<Want>, Answer) {
    match round.press(stroke) {
        Press::Answer(settled) => {
            let said = yunta_render::screen::answers_recorded(node, settled.answers.len());
            let answer = Operation::Answer {
                run: run.clone(),
                node: node.clone(),
                answers: settled.answers,
            };
            (vec![Want::Work(answer)], Answer::UnderWay(said))
        }
        Press::Interrupt => (vec![Want::Leave], Answer::Open),
        Press::Stay | Press::Moved | Press::Decline => (Vec::new(), Answer::Open),
    }
}

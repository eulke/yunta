//! A round of questions, answered in one place.
//!
//! One question is answered at a time, the way its own type is answered: typed for
//! `text`, picked from the declared values for `choice`, picked from two for `boolean`.
//! What was answered and what is to come stand around it, and a person can go back.
//!
//! Nothing is recorded until the whole round is answered: a question whose answer
//! breaks its own rules is refused right there, with what is wrong under it. Those
//! rules are the engine's, asked of the engine: the round is validated again when it is
//! submitted, and a surface that judged by its own rules would be telling a person
//! something the engine might refuse.

use surco::components::{Card, RoundRow, RoundStanding, TypedRow};
use surco::doc::Block;
use surco::look::Look;
use yunta_core::{Answer, AnswerType, NodeId, Question, QuestionsFile};
use yunta_render::prompt::round::{self, SKIP};
use yunta_render::prompt::{Escape, ANSWER};

use super::{Alternative, Answerable, Edit, Picker, Press, Stroke, Typed, TypedLine};

/// What answering a round came to: the answers that stand, each question with what a
/// reader reads as its answer, and whether an answer was pasted over more than one line
/// and folded into one.
#[derive(Debug, Default, PartialEq)]
pub struct SettledAnswers {
    pub answers: Vec<Answer>,
    /// Each question with what a reader reads as its answer, in order.
    pub said: Vec<(String, String)>,
    /// Whether an answer was pasted over more than one line and folded into one.
    pub folded: bool,
}

/// How the question being answered is answered.
#[derive(Debug, Clone)]
enum Answering {
    Typing(TypedLine),
    Picking(Picker<Option<String>>),
}

/// What a question was answered with.
#[derive(Debug, Clone)]
struct Given {
    /// The value it records, or `None` for an optional one left without.
    value: Option<String>,
    /// The line it was typed as, which a person who comes back to it finds. Nothing for
    /// an answer picked from a list.
    line: String,
    /// Whether it was pasted over more than one line and folded into one.
    folded: bool,
}

impl Given {
    /// What a typed line is given as: an empty line is no answer.
    fn typed(typed: Typed) -> Self {
        Self {
            value: (!typed.value.is_empty()).then_some(typed.value),
            line: typed.line,
            folded: typed.folded,
        }
    }
}

/// One question of the round, with what was given for it where it has been.
#[derive(Debug, Clone)]
struct Step {
    question: Question,
    given: Option<Given>,
}

/// A round of questions being answered, one at a time.
#[derive(Debug, Clone)]
pub struct Round {
    node: NodeId,
    steps: Vec<Step>,
    /// The question being answered.
    at: usize,
    answering: Answering,
    /// Why the engine did not take the last answer to this question.
    refused: Vec<String>,
    escape: Escape,
}

impl Round {
    /// A round of `questions` with the first of them being answered and nothing given.
    /// A round of no questions is never held: a surface answers it with nothing.
    pub fn new(node: &NodeId, questions: &QuestionsFile, escape: Escape) -> Self {
        let steps: Vec<Step> = questions
            .questions
            .iter()
            .map(|question| Step {
                question: question.clone(),
                given: None,
            })
            .collect();
        Self {
            node: node.clone(),
            answering: answering_at(&steps, 0, escape),
            steps,
            at: 0,
            refused: Vec::new(),
            escape,
        }
    }

    /// The question being answered, from what was given for it, as it is answered now.
    fn answering_now(&self) -> Answering {
        answering_at(&self.steps, self.at, self.escape)
    }

    /// Goes back to the question before, with what was said for it held in place. At
    /// the first question there is nothing to go back to, and it stays where it is.
    fn back(&mut self) -> Press<SettledAnswers> {
        if self.at == 0 {
            return Press::Stay;
        }
        self.at -= 1;
        self.refused.clear();
        self.answering = self.answering_now();
        Press::Moved
    }

    /// What a stroke other than going back asks of the question being answered.
    fn stroke(&mut self, stroke: Stroke) -> Press<Given> {
        match &mut self.answering {
            Answering::Typing(field) => match field.apply(stroke) {
                Edit::Editing => Press::Stay,
                Edit::Answered(typed) => Press::Answer(Given::typed(typed)),
                Edit::Declined => Press::Decline,
                Edit::Interrupted => Press::Interrupt,
            },
            Answering::Picking(list) => match list.press(stroke) {
                Press::Stay | Press::Moved => Press::Stay,
                Press::Answer(value) => Press::Answer(Given {
                    value,
                    line: String::new(),
                    folded: false,
                }),
                Press::Decline => Press::Decline,
                Press::Interrupt => Press::Interrupt,
            },
        }
    }

    /// Takes an answer to the question being answered if the engine accepts it, and
    /// moves on to the next one, or ends the round with every answer on the last. What
    /// the engine refuses stays here, with what is wrong under it.
    fn settle(&mut self, given: Given) -> Press<SettledAnswers> {
        let Some(step) = self.steps.get(self.at) else {
            return Press::Stay;
        };
        let candidate = given.value.as_ref().map(|value| Answer {
            id: step.question.id.clone(),
            value: value.clone(),
        });
        let broken = violations(&step.question, candidate.as_ref());
        if !broken.is_empty() {
            self.refused = broken;
            if matches!(self.answering, Answering::Picking(_)) {
                // The list gave up the option picked, so it is built again where it was.
                self.answering = self.answering_now();
            }
            return Press::Stay;
        }
        if let Some(step) = self.steps.get_mut(self.at) {
            step.given = Some(given);
        }
        self.refused.clear();
        if self.at + 1 == self.steps.len() {
            return Press::Answer(self.settled());
        }
        self.at += 1;
        self.answering = self.answering_now();
        Press::Moved
    }

    /// What the round holds once every question is answered: the answers that stand, in
    /// order, and each question said as a reader reads its answer.
    fn settled(&self) -> SettledAnswers {
        let mut settled = SettledAnswers::default();
        for Step { question, given } in &self.steps {
            let Some(given) = given else { continue };
            settled.said.push((
                question.text.clone(),
                said_of(question, given.value.as_deref()),
            ));
            if let Some(value) = &given.value {
                settled.answers.push(Answer {
                    id: question.id.clone(),
                    value: value.clone(),
                });
            }
            settled.folded |= given.folded;
        }
        settled
    }

    /// The round as a card: its question, what it needs, every question in its place,
    /// and the keys it answers to.
    fn card(&self) -> Card {
        let rows = self
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| self.row(index, step))
            .collect();
        let picking = matches!(self.answering, Answering::Picking(_));
        round::card(
            &self.node,
            rows,
            round::legend(picking, self.at > 0, self.escape),
        )
    }

    /// One question as its row of the round.
    fn row(&self, index: usize, step: &Step) -> RoundRow {
        RoundRow {
            asked: step.question.text.clone(),
            note: round::note(&step.question),
            stands: self.stands(index, step),
        }
    }

    /// Where the question `index` stands: being answered, answered, or still to come.
    fn stands(&self, index: usize, step: &Step) -> RoundStanding {
        if index == self.at {
            return self.answering_stands();
        }
        match &step.given {
            Some(given) => RoundStanding::Answered(said_of(&step.question, given.value.as_deref())),
            None => RoundStanding::Ahead,
        }
    }

    /// The question being answered, as what is typed in its field or the list it is
    /// picked from, with what the engine refused under it.
    fn answering_stands(&self) -> RoundStanding {
        match &self.answering {
            Answering::Typing(field) => {
                let (before, after) = field.split();
                RoundStanding::Typing {
                    typed: TypedRow {
                        prompt: ANSWER.to_string(),
                        before,
                        after,
                    },
                    refused: self.refused.clone(),
                }
            }
            Answering::Picking(list) => RoundStanding::Picking {
                choices: list.list(),
                filter: list.filter().map(str::to_string),
                refused: self.refused.clone(),
            },
        }
    }
}

impl Answerable for Round {
    type Answer = SettledAnswers;

    /// The round as its card.
    fn block(&self, _look: &Look) -> Block {
        Block::Card(self.card())
    }

    /// Shift-tab goes back; every other stroke goes to the question being answered, and
    /// what it answers is settled.
    fn press(&mut self, stroke: Stroke) -> Press<SettledAnswers> {
        match stroke {
            Stroke::BackTab => self.back(),
            stroke => match self.stroke(stroke) {
                Press::Stay | Press::Moved => {
                    self.refused.clear();
                    Press::Stay
                }
                Press::Answer(given) => self.settle(given),
                Press::Decline => Press::Decline,
                Press::Interrupt => Press::Interrupt,
            },
        }
    }

    /// Whether keys are the words of a field, rather than the keys of a list.
    fn typing(&self) -> bool {
        matches!(self.answering, Answering::Typing(_))
    }
}

/// How the question at `at` is answered. A position past the last question is never
/// reached: the round ends on its last answer, so the empty field is only a value to hold.
fn answering_at(steps: &[Step], at: usize, escape: Escape) -> Answering {
    steps.get(at).map_or_else(
        || Answering::Typing(TypedLine::default()),
        |step| answering_of(step, escape),
    )
}

/// How the question of `step` is answered, with what was given for it held in place.
fn answering_of(step: &Step, escape: Escape) -> Answering {
    let given = step.given.as_ref();
    match step.question.answer_type {
        AnswerType::Text => {
            let held = given.map_or("", |given| given.line.as_str());
            Answering::Typing(TypedLine::holding(held))
        }
        AnswerType::Choice | AnswerType::Boolean => {
            let offered = options(&step.question);
            let pointed = given.and_then(|given| {
                offered
                    .iter()
                    .position(|option| option.value == given.value)
            });
            let picker = Picker::new(offered, escape);
            Answering::Picking(match pointed {
                Some(index) => picker.pointing_at(index),
                None => picker,
            })
        }
    }
}

/// The options a list question offers: its declared values, or the two a boolean has,
/// and the option that leaves it no answer when it is not required.
fn options(question: &Question) -> Vec<Alternative<Option<String>>> {
    let mut offered = match question.answer_type {
        AnswerType::Boolean => Vec::from([true, false].map(shown)),
        _ => values(question),
    };
    if !question.required {
        offered.push(Alternative::named(SKIP, None));
    }
    offered
}

/// What a reader reads as the answer to `question`: the name of the option it was picked
/// by, or the value as it was typed.
fn said_of(question: &Question, value: Option<&str>) -> String {
    options(question)
        .into_iter()
        .find(|option| option.value.as_deref() == value)
        .map_or_else(|| value.unwrap_or(SKIP).to_string(), |option| option.name)
}

/// The values a `choice` question declares, each as itself.
fn values(question: &Question) -> Vec<Alternative<Option<String>>> {
    question
        .values
        .iter()
        .map(|value| Alternative::named(value.as_str(), Some(value.clone())))
        .collect()
}

/// One of the two answers a `boolean` question takes: read as a word,
/// recorded as the literal the artifact carries.
fn shown(value: bool) -> Alternative<Option<String>> {
    Alternative::named(round::boolean(value), Some(value.to_string()))
}

/// What the engine's own rules say about this one answer.
///
/// The question is put to [`AnswersFile::against`](yunta_core::AnswersFile::against) on its own, so the
/// sentence a person reads while answering is the very verdict the
/// round is judged by when it is submitted, never a second opinion
/// written here that the engine might not share.
fn violations(question: &Question, answer: Option<&Answer>) -> Vec<String> {
    let alone = QuestionsFile {
        questions: vec![question.clone()],
    };
    match yunta_core::AnswersFile::against(&alone, answer.cloned().into_iter().collect()) {
        Ok(_) => Vec::new(),
        Err(report) => report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests;

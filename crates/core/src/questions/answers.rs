//! The `kind: answers` artifact: what a person replied, as a document
//! the run holds and the next node reads.
//!
//! The engine writes it — a reply arrives through `yunta answer` or a
//! console form, never from an agent — and reads it back through the
//! same door every other document goes through, so a node that mounts
//! `kind: answers` meets the shape that was checked rather than a file
//! somebody wrote beside it.
//!
//! Two frontiers, not one. [`read`](crate::shape::read) sees this
//! document alone: it settles the keys, and the one rule that spans it —
//! an id answered twice. What a reply owes the questions it answers is
//! [`AnswersFile::against`], because the questions are a second document
//! and a door that reads one cannot see the other.

use crate::diagnostic::{Diagnostic, Named, Problem, Report, Rule, RuleCode, Subject};
use crate::questions::{Answer, AnswerType, AnswersFile, QuestionsFile};
use crate::shape::Document;
use crate::ArtifactKind;

/// The shape this document publishes, as the YAML it is.
///
/// It lives as a file rather than a string literal, so an editor reads
/// it as YAML and a person reviewing a schema change sees the diff in
/// the format the change is about.
const EXAMPLE: &str = include_str!("answers.yaml");

/// Every rule this document is held to — see `crate::tasks::rules` for
/// what this list is for and what holds it true.
const RULES: &[Rule] = &[
    Rule {
        code: RuleCode::DuplicateId,
        demand: "each `id` answers once: one question has one answer",
    },
    Rule {
        code: RuleCode::UnknownId,
        demand: "each `id` names a question the document being answered asked",
    },
    Rule {
        code: RuleCode::MismatchedAnswer,
        demand: "a `choice` answer is one of that question's `values`, and a `boolean` one is \
                 `true` or `false`",
    },
    Rule {
        code: RuleCode::MissingAnswer,
        demand: "every `required` question is answered",
    },
];

impl Document for AnswersFile {
    const KIND: ArtifactKind = ArtifactKind::Answers;
    const EXAMPLE: &'static str = EXAMPLE;

    fn check(&self) -> Vec<Diagnostic> {
        let mut broken = Vec::new();
        let mut answered = std::collections::HashSet::new();
        for (index, answer) in self.answers.iter().enumerate() {
            if !answered.insert(&answer.id) {
                broken.push(Diagnostic::new(
                    Subject::Question(Named::new(answer.id.clone(), index)),
                    Problem::rule(
                        RuleCode::DuplicateId,
                        "this question is already answered above; one question has one answer",
                    ),
                ));
            }
        }
        broken
    }

    const RULES: &'static [Rule] = RULES;
}

impl AnswersFile {
    /// The answers `given` are, judged against the questions they claim
    /// to answer — or every way they fail to.
    ///
    /// The frontier [`read`](crate::shape::read) cannot cross: it reads
    /// one document against its own type, and what makes a reply an
    /// answer is a second document. Every `required` question is
    /// answered, every answer names a question that exists, a `choice`
    /// carries one of the values declared for it, and a `boolean`
    /// carries one of the two words. Every violation together, never
    /// the first, so one correction closes them all.
    pub fn against(questions: &QuestionsFile, given: Vec<Answer>) -> Result<Self, Report> {
        let mut broken: Vec<Diagnostic> = given
            .iter()
            .enumerate()
            .filter_map(|(index, answer)| answers_its_question(questions, index, answer))
            .chain(
                questions
                    .questions
                    .iter()
                    .enumerate()
                    .filter(|(_, question)| {
                        question.required && !given.iter().any(|answer| answer.id == question.id)
                    })
                    .map(|(index, question)| {
                        broke(
                            index,
                            &question.id,
                            RuleCode::MissingAnswer,
                            "this question is `required` and nothing answers it".to_string(),
                        )
                    }),
            )
            .collect();
        let file = AnswersFile { answers: given };
        broken.extend(file.check());
        if broken.is_empty() {
            Ok(file)
        } else {
            Err(Report::new(
                crate::diagnostic::DocumentRef::new(ArtifactKind::Answers, "answers.yaml"),
                broken,
            ))
        }
    }
}

/// How one answer fails the question it names, or `None` when it
/// answers it.
fn answers_its_question(
    questions: &QuestionsFile,
    index: usize,
    answer: &Answer,
) -> Option<Diagnostic> {
    let Some(question) = questions.questions.iter().find(|q| q.id == answer.id) else {
        return Some(broke(
            index,
            &answer.id,
            RuleCode::UnknownId,
            "no question carries this id; an answer answers a question that was asked".to_string(),
        ));
    };
    let mismatch = match question.answer_type {
        AnswerType::Choice if !question.values.contains(&answer.value) => format!(
            "`{}` is not one of the values this question allows: {}",
            answer.value,
            question.values.join(", ")
        ),
        AnswerType::Boolean if answer.value != "true" && answer.value != "false" => {
            format!("`{}` is not `true` or `false`", answer.value)
        }
        AnswerType::Text | AnswerType::Choice | AnswerType::Boolean => return None,
    };
    Some(broke(
        index,
        &answer.id,
        RuleCode::MismatchedAnswer,
        mismatch,
    ))
}

fn broke(index: usize, id: &crate::QuestionId, code: RuleCode, detail: String) -> Diagnostic {
    Diagnostic::new(
        Subject::Question(Named::new(id.clone(), index)),
        Problem::rule(code, detail),
    )
}

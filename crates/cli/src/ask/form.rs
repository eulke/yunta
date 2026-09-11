//! The sequence a `kind: questions` artifact is answered through.
//!
//! A sequence and not a screen: one question is on at a time, answered
//! the way its own type is answered — typed for `text`, picked from the
//! declared values for `choice`, picked from two for `boolean`. The
//! header says how many answers the round needs, since the questions
//! that follow cannot all be shown at once.
//!
//! Nothing is recorded until the whole round is answered: a question
//! whose answer breaks its own rules is asked again, right there, with
//! what is wrong above it. Those rules are the engine's, asked of the
//! engine — the round is validated again when it is submitted, and a
//! surface that judged by its own rules would be telling a person
//! something the engine might refuse.

use yunta_core::events::Channel;
use yunta_core::{Answer, AnswerType, Question, QuestionsFile};
use yunta_engine::QuestionsReply;

use super::field::ask_line;
use super::menu::{choose, Choice};
use super::{attributed, Answered, Console, ANSWER, PARKS};

/// The option that leaves a question no answer, offered only where the
/// question allows one, and what stands for the answer that was not
/// given when the choice is echoed back.
const SKIP: &str = "(no answer)";

/// Puts every question in `questions` to the person, in order.
pub(crate) fn answer(console: &Console, questions: &QuestionsFile) -> Answered<QuestionsReply> {
    let total = questions.questions.len();
    console.say("")?;
    console.say(&format!(
        "{total} {} needed before this node goes on ({PARKS})",
        if total == 1 { "answer" } else { "answers" }
    ))?;
    let mut answers = Vec::new();
    for (index, question) in questions.questions.iter().enumerate() {
        console.say("")?;
        console.say(&format!(
            "{}/{total}  {}{}",
            index + 1,
            question.text,
            if question.required { "" } else { " (optional)" }
        ))?;
        answers.extend(asked(console, question)?);
    }
    Ok(QuestionsReply {
        answers,
        channel: Channel::Tty,
        responder: Some(attributed(console)?),
    })
}

/// One question, asked until it has an answer its own rules accept.
fn asked(console: &Console, question: &Question) -> Answered<Option<Answer>> {
    loop {
        let candidate = match question.answer_type {
            AnswerType::Text => typed(console)?,
            AnswerType::Choice => picked(console, question, values(question))?,
            AnswerType::Boolean => picked(console, question, [true, false].map(shown).into())?,
        }
        .map(|value| Answer {
            id: question.id.clone(),
            value,
        });
        let broken = violations(question, candidate.as_ref());
        if broken.is_empty() {
            return Ok(candidate);
        }
        for violation in broken {
            console.say(&violation)?;
        }
    }
}

/// A typed answer, or none where an empty line was left.
fn typed(console: &Console) -> Answered<Option<String>> {
    let value = ask_line(console, ANSWER)?.value;
    Ok((!value.is_empty()).then_some(value))
}

/// An answer picked off `choices`, echoed as the value it records
/// rather than the label it was picked by.
fn picked(
    console: &Console,
    question: &Question,
    mut choices: Vec<Choice<Option<String>>>,
) -> Answered<Option<String>> {
    if !question.required {
        choices.push(Choice {
            head: SKIP.to_string(),
            detail: None,
            value: None,
        });
    }
    let value = choose(console, "answer", choices)?;
    console.say(&format!("{ANSWER}{}", value.as_deref().unwrap_or(SKIP)))?;
    Ok(value)
}

/// The values a `choice` question declares, each as itself.
fn values(question: &Question) -> Vec<Choice<Option<String>>> {
    question
        .values
        .iter()
        .map(|value| Choice {
            head: value.clone(),
            detail: None,
            value: Some(value.clone()),
        })
        .collect()
}

/// One of the two answers a `boolean` question takes: read as a word,
/// recorded as the literal the artifact carries.
fn shown(value: bool) -> Choice<Option<String>> {
    Choice {
        head: if value { "yes" } else { "no" }.to_string(),
        detail: None,
        value: Some(value.to_string()),
    }
}

/// What the engine's own rules say about this one answer.
///
/// The question is put to `validate_answers` on its own, so the
/// sentence a person reads while answering is the very verdict the
/// round is judged by when it is submitted, never a second opinion
/// written here that the engine might not share.
fn violations(question: &Question, answer: Option<&Answer>) -> Vec<String> {
    let alone = QuestionsFile {
        questions: vec![question.clone()],
    };
    yunta_core::validate_answers(&alone, answer.map(std::slice::from_ref).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(answer_type: AnswerType, required: bool) -> Question {
        Question {
            id: "env".into(),
            text: "Which environment?".to_string(),
            answer_type,
            values: vec!["staging".to_string(), "production".to_string()],
            required,
        }
    }

    #[test]
    fn a_required_question_left_unanswered_is_the_engines_own_refusal() {
        let broken = violations(&question(AnswerType::Text, true), None);
        assert_eq!(
            broken,
            yunta_core::validate_answers(
                &QuestionsFile {
                    questions: vec![question(AnswerType::Text, true)]
                },
                &[]
            )
        );
        assert!(!broken.is_empty(), "the round is not complete without it");
    }

    #[test]
    fn an_optional_question_left_unanswered_breaks_nothing() {
        assert!(violations(&question(AnswerType::Choice, false), None).is_empty());
    }

    #[test]
    fn a_declared_value_is_what_a_choice_records() {
        let offered: Vec<Option<String>> = values(&question(AnswerType::Choice, true))
            .into_iter()
            .map(|choice| choice.value)
            .collect();
        assert_eq!(
            offered,
            vec![Some("staging".to_string()), Some("production".to_string())]
        );
    }

    #[test]
    fn a_boolean_is_read_as_a_word_and_recorded_as_a_literal() {
        let yes = shown(true);
        assert_eq!(yes.head, "yes");
        assert_eq!(yes.value, Some("true".to_string()));
        let answered = Answer {
            id: "env".into(),
            value: yes.value.unwrap_or_default(),
        };
        assert!(violations(&question(AnswerType::Boolean, true), Some(&answered)).is_empty());
    }
}

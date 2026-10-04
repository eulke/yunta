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

use crate::render::blocks::{Drawn, Headline};
use crate::render::ink::{Line, Tone};
use crate::render::{Mark, INDENT};

use super::field::ask_line;
use super::menu::{choose, Choice};
use super::{attributed, Answered, Console, ANSWER};

/// The option that leaves a question no answer, offered only where the
/// question allows one, and what stands for the answer that was not
/// given when the choice is echoed back.
const SKIP: &str = "(no answer)";

/// Puts every question `node` asks in `questions` to the person, in
/// order, under a headline that says who is asking and how many
/// answers the round needs.
pub(crate) fn answer(
    console: &Console,
    node: &yunta_core::NodeId,
    questions: &QuestionsFile,
) -> Answered<QuestionsReply> {
    let total = questions.questions.len();
    let look = console.look();
    let headline = Headline {
        subject: format!("node `{node}`"),
        mark: Mark::NeedsYou,
        said: "needs you".to_string(),
    };
    console.say("")?;
    for line in headline.lines(&look) {
        console.say(&look.ink.paint(&line))?;
    }
    console.say(&look.ink.paint(&Line::new().plain(INDENT).plain(format!(
        "{} before it goes on",
        yunta_core::text::counted(total, "answer")
    ))))?;
    console.say(
        &look.ink.paint(
            &Line::new()
                .plain(INDENT)
                .push(Tone::Muted, console.escape().said()),
        ),
    )?;
    let mut answers = Vec::new();
    for (index, question) in questions.questions.iter().enumerate() {
        console.say("")?;
        let mut asking = Line::new()
            .push(Tone::Muted, format!("{}/{total}", index + 1))
            .plain("  ")
            .push(Tone::Strong, question.text.as_str());
        if !question.required {
            asking = asking.push(Tone::Muted, " (optional)");
        }
        console.say(&look.ink.paint(&asking))?;
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
        let look = console.look();
        for violation in broken {
            let line = Line::new()
                .push(Tone::Failed, look.glyphs.mark(Mark::Failed).to_string())
                .plain(" ")
                .plain(violation);
            console.say(&look.ink.paint(&line))?;
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
        choices.push(Choice::named(SKIP, None));
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
        .map(|value| Choice::named(value.as_str(), Some(value.clone())))
        .collect()
}

/// One of the two answers a `boolean` question takes: read as a word,
/// recorded as the literal the artifact carries.
fn shown(value: bool) -> Choice<Option<String>> {
    Choice::named(if value { "yes" } else { "no" }, Some(value.to_string()))
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
mod tests {
    use super::*;

    fn question(answer_type: AnswerType, required: bool) -> Question {
        Question {
            id: "env".into(),
            text: "Which environment?".to_string(),
            answer_type,
            values: vec!["staging".to_string(), "production".to_string()],
            required,
            assumes: (!required).then(|| "staging".to_string()),
        }
    }

    #[test]
    fn a_required_question_left_unanswered_is_the_engines_own_refusal() {
        let broken = violations(&question(AnswerType::Text, true), None);
        assert!(!broken.is_empty(), "the round is not complete without it");
        assert!(
            broken[0].contains("required"),
            "the engine's own words: {broken:?}"
        );
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
        assert_eq!(yes.name, "yes");
        assert_eq!(yes.value, Some("true".to_string()));
        let answered = Answer {
            id: "env".into(),
            value: yes.value.unwrap_or_default(),
        };
        assert!(violations(&question(AnswerType::Boolean, true), Some(&answered)).is_empty());
    }
}

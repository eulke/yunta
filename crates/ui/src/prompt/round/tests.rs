//! What a round of questions does with each key, with no terminal: a stroke goes in
//! and what it asks of the prompt comes out.

use surco::look::Look;
use yunta_core::{AnswerType, NodeId, Question, QuestionsFile};
use yunta_render::prompt::Escape;

use super::{violations, Round, SettledAnswers};
use crate::prompt::{Answerable, Press, Stroke};

fn asked(id: &str, text: &str, answer_type: AnswerType, required: bool) -> Question {
    Question {
        id: id.into(),
        text: text.to_string(),
        answer_type,
        values: match answer_type {
            AnswerType::Choice => vec!["staging".to_string(), "production".to_string()],
            _ => Vec::new(),
        },
        required,
        assumes: (!required).then(|| "staging".to_string()),
    }
}

fn round(questions: Vec<Question>) -> Round {
    Round::new(
        &NodeId::from_static("ask"),
        &QuestionsFile { questions },
        Escape::Parks,
    )
}

fn two_typed() -> Round {
    round(vec![
        asked("summary", "What changed?", AnswerType::Text, true),
        asked("risk", "Any risk?", AnswerType::Text, true),
    ])
}

fn rows(round: &Round) -> Vec<String> {
    held(round, usize::MAX)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

fn holds(round: &Round, said: &str) -> bool {
    rows(round).iter().any(|row| row.contains(said))
}

fn typed(round: &mut Round, words: &str) {
    for character in words.chars() {
        assert_eq!(round.press(Stroke::Insert(character)), Press::Stay);
    }
}

/// What the last answer ended the round with: each question's id and value.
fn ended(press: Press<SettledAnswers>) -> Vec<(String, String)> {
    match press {
        Press::Answer(settled) => settled
            .answers
            .into_iter()
            .map(|answer| (answer.id.to_string(), answer.value))
            .collect(),
        other => panic!("the round did not end: {other:?}"),
    }
}

fn pair(id: &str, value: &str) -> (String, String) {
    (id.to_string(), value.to_string())
}

#[test]
fn enter_answers_a_typed_question_and_moves_on_to_the_next() {
    let mut round = two_typed();
    assert!(holds(&round, "? What changed?"), "{:?}", rows(&round));
    assert!(holds(&round, ". Any risk?"), "{:?}", rows(&round));
    typed(&mut round, "abc");
    assert_eq!(round.press(Stroke::Enter), Press::Moved);
    assert!(holds(&round, "+ What changed?  abc"), "{:?}", rows(&round));
    assert!(holds(&round, "? Any risk?"), "{:?}", rows(&round));
}

#[test]
fn the_last_answer_ends_the_round_with_every_answer() {
    let mut round = two_typed();
    typed(&mut round, "abc");
    round.press(Stroke::Enter);
    typed(&mut round, "none");
    assert_eq!(
        ended(round.press(Stroke::Enter)),
        [pair("summary", "abc"), pair("risk", "none")]
    );
}

/// What a question refuses is said in the engine's own words, and the question stays.
#[test]
fn a_required_question_left_empty_is_refused_in_the_engines_words() {
    let mut round = two_typed();
    assert_eq!(round.press(Stroke::Enter), Press::Stay);
    let refusal = violations(
        &asked("summary", "What changed?", AnswerType::Text, true),
        None,
    );
    assert!(refusal[0].contains("required"), "{refusal:?}");
    assert!(holds(&round, &refusal[0]), "{:?}", rows(&round));
    assert!(holds(&round, "? What changed?"), "{:?}", rows(&round));
    typed(&mut round, "a");
    assert!(
        !holds(&round, &refusal[0]),
        "a refusal is gone once there is something new to judge: {:?}",
        rows(&round)
    );
}

#[test]
fn an_optional_question_says_what_it_assumes_and_may_be_left_empty() {
    let mut round = round(vec![asked("note", "Anything?", AnswerType::Text, false)]);
    assert!(
        holds(&round, "optional, assumes staging"),
        "{:?}",
        rows(&round)
    );
    assert_eq!(ended(round.press(Stroke::Enter)), Vec::new());
}

#[test]
fn a_choice_is_answered_by_arrow_or_by_typing() {
    let mut by_arrow = round(vec![asked("env", "Which?", AnswerType::Choice, true)]);
    assert!(holds(&by_arrow, "* staging"), "{:?}", rows(&by_arrow));
    assert_eq!(by_arrow.press(Stroke::Down), Press::Stay);
    assert_eq!(
        ended(by_arrow.press(Stroke::Enter)),
        [pair("env", "production")]
    );

    let mut by_typing = round(vec![asked("env", "Which?", AnswerType::Choice, true)]);
    typed(&mut by_typing, "prod");
    assert!(holds(&by_typing, "filter prod"), "{:?}", rows(&by_typing));
    assert_eq!(
        ended(by_typing.press(Stroke::Enter)),
        [pair("env", "production")]
    );
}

/// A boolean is read as a word and recorded as the literal the artifact carries.
#[test]
fn a_boolean_reads_as_a_word_and_is_recorded_as_a_literal() {
    let mut round = round(vec![
        asked("go", "Go on?", AnswerType::Boolean, true),
        asked("risk", "Any risk?", AnswerType::Text, true),
    ]);
    assert!(holds(&round, "* yes"), "{:?}", rows(&round));
    assert!(holds(&round, "  no"), "{:?}", rows(&round));
    assert_eq!(round.press(Stroke::Enter), Press::Moved);
    assert!(holds(&round, "+ Go on?  yes"), "{:?}", rows(&round));
    typed(&mut round, "none");
    assert_eq!(
        ended(round.press(Stroke::Enter)),
        [pair("go", "true"), pair("risk", "none")]
    );
}

/// A list leaves an optional question no answer through a row of its own.
#[test]
fn an_optional_choice_may_be_left_with_no_answer() {
    let mut round = round(vec![asked("env", "Which?", AnswerType::Choice, false)]);
    assert!(holds(&round, "(no answer)"), "{:?}", rows(&round));
    round.press(Stroke::Up);
    assert_eq!(ended(round.press(Stroke::Enter)), Vec::new());
}

/// Going back finds what was said, and what came after it stays answered.
#[test]
fn shift_tab_goes_back_with_what_was_said_and_keeps_what_came_after() {
    let mut round = round(vec![
        asked("one", "One?", AnswerType::Text, true),
        asked("two", "Two?", AnswerType::Text, true),
        asked("three", "Three?", AnswerType::Text, true),
    ]);
    typed(&mut round, "first");
    round.press(Stroke::Enter);
    typed(&mut round, "second");
    round.press(Stroke::Enter);
    assert_eq!(round.press(Stroke::BackTab), Press::Moved);
    assert_eq!(round.press(Stroke::BackTab), Press::Moved);
    assert!(holds(&round, "? One?"), "{:?}", rows(&round));
    assert!(holds(&round, "> first"), "{:?}", rows(&round));
    assert!(holds(&round, "+ Two?  second"), "{:?}", rows(&round));
    typed(&mut round, "!");
    assert_eq!(round.press(Stroke::Enter), Press::Moved);
    assert!(holds(&round, "? Two?"), "{:?}", rows(&round));
    assert!(holds(&round, "> second"), "{:?}", rows(&round));
    round.press(Stroke::Enter);
    typed(&mut round, "third");
    assert_eq!(
        ended(round.press(Stroke::Enter)),
        [
            pair("one", "first!"),
            pair("two", "second"),
            pair("three", "third")
        ]
    );
}

/// Going back to a list finds the pointer on what was picked.
#[test]
fn going_back_to_a_list_points_at_what_was_picked() {
    let mut round = round(vec![
        asked("env", "Which?", AnswerType::Choice, true),
        asked("risk", "Any risk?", AnswerType::Text, true),
    ]);
    round.press(Stroke::Down);
    round.press(Stroke::Enter);
    assert_eq!(round.press(Stroke::BackTab), Press::Moved);
    assert!(holds(&round, "* production"), "{:?}", rows(&round));
}

/// There is nothing to go back to from the first question, wherever it is answered.
#[test]
fn shift_tab_on_the_first_question_stays_there() {
    let mut typing = two_typed();
    assert_eq!(typing.press(Stroke::BackTab), Press::Stay);
    assert!(holds(&typing, "? What changed?"), "{:?}", rows(&typing));
    let mut list = round(vec![asked("env", "Which?", AnswerType::Choice, true)]);
    let before = rows(&list);
    assert_eq!(list.press(Stroke::BackTab), Press::Stay);
    assert_eq!(rows(&list), before, "the pointer did not move");
}

/// What a round says of a paste it folded is true of what it records: an answer said
/// again on one line was not folded, and neither was one the engine refused.
#[test]
fn what_was_folded_is_said_only_of_an_answer_that_stands() {
    let folded_answer = |round: &mut Round| {
        typed(round, "a");
        round.press(Stroke::PastedBreak);
        typed(round, "b");
        assert_eq!(round.press(Stroke::Enter), Press::Moved);
    };
    let settled = |press: Press<SettledAnswers>| match press {
        Press::Answer(settled) => settled.folded,
        other => panic!("the round did not end: {other:?}"),
    };

    let mut kept = two_typed();
    folded_answer(&mut kept);
    typed(&mut kept, "none");
    assert!(
        settled(kept.press(Stroke::Enter)),
        "it was folded and it stands"
    );

    let mut replaced = two_typed();
    folded_answer(&mut replaced);
    replaced.press(Stroke::BackTab);
    for _ in 0.."a b".len() {
        replaced.press(Stroke::Backspace);
    }
    typed(&mut replaced, "x");
    assert_eq!(replaced.press(Stroke::Enter), Press::Moved);
    typed(&mut replaced, "none");
    assert!(
        !settled(replaced.press(Stroke::Enter)),
        "said again on one line"
    );

    let mut confirmed = two_typed();
    folded_answer(&mut confirmed);
    confirmed.press(Stroke::BackTab);
    assert!(holds(&confirmed, "> a b"), "{:?}", rows(&confirmed));
    assert_eq!(confirmed.press(Stroke::Enter), Press::Moved);
    typed(&mut confirmed, "none");
    assert!(
        settled(confirmed.press(Stroke::Enter)),
        "coming back to it and pressing enter changed nothing about it"
    );
}

#[test]
fn esc_leaves_the_round_and_ctrl_c_stops_the_run_from_a_field_and_from_a_list() {
    let mut typing = two_typed();
    assert_eq!(typing.press(Stroke::Interrupt), Press::Interrupt);
    assert_eq!(two_typed().press(Stroke::Decline), Press::Decline);
    let list = || round(vec![asked("env", "Which?", AnswerType::Choice, true)]);
    assert_eq!(list().press(Stroke::Interrupt), Press::Interrupt);
    assert_eq!(list().press(Stroke::Decline), Press::Decline);
}

/// A paste is an answer only to what is typed.
#[test]
fn only_a_typed_question_takes_a_paste() {
    assert!(two_typed().typing());
    assert!(!round(vec![asked("env", "Which?", AnswerType::Choice, true)]).typing());
}

/// A round says how many answers it needs, whose they are, and that nothing is kept
/// until all of them are given; a single question has nothing else to wait for.
#[test]
fn a_round_says_what_it_needs_and_that_nothing_is_recorded_until_the_end() {
    let drawn = rows(&two_typed());
    assert!(
        drawn[0].contains("2 answers before `ask` goes on"),
        "{drawn:?}"
    );
    assert!(
        drawn[1].contains("nothing is recorded until every one is answered"),
        "{drawn:?}"
    );
    let one = rows(&round(vec![asked("a", "One?", AnswerType::Text, true)]));
    assert!(one[0].contains("1 answer before `ask` goes on"), "{one:?}");
    assert!(
        !one.iter().any(|row| row.contains("nothing is recorded")),
        "{one:?}"
    );
}

/// Going back is on offer only where there is somewhere to go back to, and a list says
/// what a field has no use for.
#[test]
fn the_keys_say_what_each_question_answers_to() {
    let mut round = round(vec![
        asked("a", "One?", AnswerType::Text, true),
        asked("env", "Which?", AnswerType::Choice, true),
    ]);
    let keys = |round: &Round| rows(round).join(" ");
    assert!(
        !keys(&round).contains("shift-tab goes back"),
        "{}",
        keys(&round)
    );
    assert!(!keys(&round).contains("type to filter"), "{}", keys(&round));
    assert!(
        keys(&round).contains("esc leaves it for later"),
        "{}",
        keys(&round)
    );
    typed(&mut round, "x");
    round.press(Stroke::Enter);
    assert!(
        keys(&round).contains("shift-tab goes back"),
        "{}",
        keys(&round)
    );
    assert!(keys(&round).contains("type to filter"), "{}", keys(&round));
}

/// What the engine's own rules say of an answer, asked of the engine.
mod verdicts {
    use super::super::*;

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
            "the engine's own words: {}",
            broken.join(" ")
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

/// `prompt` as a terminal of `rows` rows holds it, at the measure of a line.
fn held(prompt: &impl Answerable, rows: usize) -> surco::surface::Placed {
    let look = Look::plain();
    surco::surface::Terminal::on(look).held(&prompt.block(&look), look.width.cells(), rows)
}

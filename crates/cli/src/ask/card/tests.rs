//! What a decision does with each key, with no terminal: a stroke goes in and
//! what it asks of the prompt comes out.

use std::collections::BTreeMap;

use surco::look::Look;
use yunta_core::events::{AuthorGate, Claim, Escalation, GateOption, Leads, Menu};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, OptionId};
use yunta_render::gate::Route;

use super::super::held::{Held, Press};
use super::super::keys::Stroke;
use super::super::Escape;
use super::{Answer, DecisionCard};

/// A gate offering to go on, to go back with words, and to pause.
fn card() -> DecisionCard {
    let escalation = Escalation::new(
        Claim::NeedsDecision {
            node: NodeId::from_static("ok"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        }),
        NonEmpty::from((
            GateOption::leading(
                OptionId::from_static("approve"),
                Leads::On {
                    next: vec![NodeId::from_static("ship")],
                    settles: false,
                },
            ),
            vec![
                GateOption::leading(
                    OptionId::from_static("adjust"),
                    Leads::Back {
                        node: NodeId::from_static("plan"),
                        resumes: true,
                    },
                ),
                GateOption::reserved(ReservedOption::Abort),
            ],
        )),
    )
    .into_payload();
    let routes = BTreeMap::from([
        (
            OptionId::from_static("approve"),
            Route::On(vec![vec![NodeId::from_static("ship")]]),
        ),
        (
            OptionId::from_static("adjust"),
            Route::Back(vec![
                vec![NodeId::from_static("plan")],
                vec![NodeId::from_static("ok")],
            ]),
        ),
    ]);
    DecisionCard::new(&escalation, Escape::Parks, routes)
}

fn answer(option: &'static str, words: Option<&str>) -> Press<Answer> {
    Press::Answer(Answer {
        option: OptionId::from_static(option),
        words: words.map(str::to_string),
        folded: false,
    })
}

fn typed(card: &mut DecisionCard, words: &str) {
    for character in words.chars() {
        assert_eq!(card.press(Stroke::Insert(character)), Press::Stay);
    }
}

/// The row the pointer stands on, and every row of the card.
fn rows(card: &DecisionCard) -> Vec<String> {
    card.drawn(&Look::plain(), usize::MAX)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

fn pointed(card: &DecisionCard) -> String {
    rows(card)
        .into_iter()
        .find(|row| row.contains("* "))
        .unwrap_or_default()
}

#[test]
fn enter_takes_the_option_pointed_at_with_one_key() {
    assert_eq!(card().press(Stroke::Enter), answer("approve", None));
}

#[test]
fn arrows_and_letters_move_the_pointer_and_it_goes_around() {
    let mut card = card();
    assert_eq!(card.press(Stroke::Down), Press::Stay);
    assert!(pointed(&card).contains("adjust"), "{:?}", rows(&card));
    assert_eq!(card.press(Stroke::Insert('j')), Press::Stay);
    assert!(pointed(&card).contains("abort"), "{:?}", rows(&card));
    assert_eq!(card.press(Stroke::Down), Press::Stay);
    assert!(pointed(&card).contains("approve"), "{:?}", rows(&card));
    assert_eq!(card.press(Stroke::Up), Press::Stay);
    assert!(pointed(&card).contains("abort"), "{:?}", rows(&card));
    assert_eq!(card.press(Stroke::Insert('k')), Press::Stay);
    assert!(pointed(&card).contains("adjust"), "{:?}", rows(&card));
}

/// Every option says what it does and what choosing it costs, in one sentence,
/// wherever the pointer stands.
#[test]
fn every_option_says_what_it_does_and_what_it_costs() {
    let rows = rows(&card());
    for said in [
        "approve  On to `ship`: resolves this gate",
        "abort    Pause here: nothing runs; this decision stays open",
    ] {
        assert!(rows.iter().any(|row| row.contains(said)), "{rows:?}");
    }
}

/// An option whose words are its id is named once.
#[test]
fn an_option_whose_words_are_its_id_is_named_once() {
    let escalation = Escalation::new(
        Claim::NeedsDecision {
            node: NodeId::from_static("ok"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        }),
        NonEmpty::from((GateOption::reserved(ReservedOption::Approve), Vec::new())),
    )
    .into_payload();
    let card = DecisionCard::new(&escalation, Escape::Parks, BTreeMap::new());
    let rows = rows(&card);
    assert!(
        rows.iter().any(|row| row.ends_with("* approve")),
        "{rows:?}"
    );
}

/// What follows is the way of the option pointed at, and an option with none says
/// no `then`.
#[test]
fn then_follows_the_pointer() {
    let mut card = card();
    assert!(rows(&card).iter().any(|row| row.ends_with("then  ship")));
    card.press(Stroke::Down);
    assert!(
        rows(&card)
            .iter()
            .any(|row| row.ends_with("then  plan -> ? ok")),
        "{:?}",
        rows(&card)
    );
    card.press(Stroke::Down);
    assert!(!rows(&card).iter().any(|row| row.contains("then ")));
}

/// An option that asks for words opens a field, and an empty answer is said back
/// rather than sent.
#[test]
fn an_option_that_asks_refuses_an_empty_answer_and_sends_what_is_typed() {
    let mut card = card();
    card.press(Stroke::Down);
    assert_eq!(card.press(Stroke::Enter), Press::Stay);
    assert!(card.typing());
    assert!(rows(&card)
        .iter()
        .any(|row| row.contains("what should change?")));
    assert_eq!(card.press(Stroke::Enter), Press::Stay);
    assert!(
        rows(&card)
            .iter()
            .any(|row| row.contains("this option needs an answer")),
        "{:?}",
        rows(&card)
    );
    typed(&mut card, "keep the lock");
    assert!(
        !rows(&card)
            .iter()
            .any(|row| row.contains("needs an answer")),
        "a refusal is gone once there is something to send: {:?}",
        rows(&card)
    );
    assert_eq!(
        card.press(Stroke::Enter),
        answer("adjust", Some("keep the lock"))
    );
}

/// `n` opens the same field for a note on an option that asks nothing, and a note
/// left blank records the decision as it stands.
#[test]
fn n_adds_a_note_and_a_blank_note_is_no_note() {
    let mut card = self::card();
    assert_eq!(card.press(Stroke::Insert('n')), Press::Stay);
    assert!(card.typing());
    typed(&mut card, "after lunch");
    assert_eq!(
        card.press(Stroke::Enter),
        answer("approve", Some("after lunch"))
    );

    let mut card = self::card();
    card.press(Stroke::Insert('N'));
    assert_eq!(card.press(Stroke::Enter), answer("approve", None));
}

/// `n` on an option that asks for words is the question it asks.
#[test]
fn n_on_an_option_that_asks_opens_what_it_asks() {
    let mut card = card();
    card.press(Stroke::Down);
    assert_eq!(card.press(Stroke::Insert('n')), Press::Stay);
    assert!(rows(&card)
        .iter()
        .any(|row| row.contains("what should change?")));
}

/// Esc in a field goes back to the list with the pointer where it was; esc in the
/// list leaves the decision.
#[test]
fn esc_goes_back_from_a_field_and_leaves_from_the_list() {
    let mut card = card();
    card.press(Stroke::Down);
    card.press(Stroke::Enter);
    typed(&mut card, "half a thought");
    assert_eq!(card.press(Stroke::Decline), Press::Stay);
    assert!(!card.typing());
    assert!(pointed(&card).contains("adjust"), "{:?}", rows(&card));
    assert_eq!(card.press(Stroke::Decline), Press::Decline);
}

/// Words typed and left are not kept for the next time the field opens.
#[test]
fn a_field_opens_empty_after_it_was_left() {
    let mut card = card();
    card.press(Stroke::Down);
    card.press(Stroke::Enter);
    typed(&mut card, "half a thought");
    card.press(Stroke::Decline);
    card.press(Stroke::Enter);
    assert_eq!(card.press(Stroke::Enter), Press::Stay);
    assert!(rows(&card)
        .iter()
        .any(|row| row.contains("needs an answer")));
}

#[test]
fn ctrl_c_stops_the_run_from_the_list_and_from_a_field() {
    let mut card = self::card();
    assert_eq!(card.press(Stroke::Interrupt), Press::Interrupt);
    let mut card = self::card();
    card.press(Stroke::Insert('n'));
    assert_eq!(card.press(Stroke::Interrupt), Press::Interrupt);
}

/// A key the list has no use for moves nothing.
#[test]
fn a_key_with_no_meaning_changes_nothing() {
    let mut card = card();
    let before = rows(&card);
    assert_eq!(card.press(Stroke::Insert('x')), Press::Stay);
    assert_eq!(card.press(Stroke::Ignored), Press::Stay);
    assert_eq!(rows(&card), before);
}

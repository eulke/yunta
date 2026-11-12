//! How a decision turns between its card and the plan behind it, with no terminal: a
//! stroke goes in, and what it came to and the one in front come out.

use surco::doc::Block;
use yunta_core::shown::PlanReview;
use yunta_core::OptionId;
use yunta_render::page::desk::Reading;

use super::{Deciding, InFront, Opens};
use crate::prompt::decision_card::tests::card;
use crate::prompt::plan_desk::tests::{look, review};
use crate::prompt::{Answerable, Asked, PickedOption, Press, Stroke};

/// The gate of the card's tests, on the plan of the desk's tests, opening on `opens`.
fn deciding(review: &PlanReview, opens: Opens) -> Deciding {
    let asked = Asked {
        question: "Plan registered. Approve?".to_string(),
        node: Some("approve-plan".to_string()),
        run: Some("EKPBGE".to_string()),
        called: Some("harvest (standard)".to_string()),
    };
    Deciding::on_plan(
        card(),
        Reading::of(review.clone(), Vec::new()),
        asked,
        opens,
    )
}

fn shows_the_plan(deciding: &mut Deciding) -> bool {
    let block_is_the_desk = matches!(deciding.block(&look()), Block::Desk(_));
    let held_is_the_desk = matches!(deciding.in_front(), InFront::ThePlan(_));
    assert_eq!(block_is_the_desk, held_is_the_desk, "drawn and held agree");
    held_is_the_desk
}

/// A person who came to read the plan reads it first, and leaving it leaves the decision
/// waiting, as Escape does at the card.
#[test]
fn opening_on_the_plan_shows_it_and_leaving_it_declines() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    assert!(shows_the_plan(&mut deciding));
    assert_eq!(deciding.press(Stroke::Decline), Press::Decline);
}

/// `r` at the card turns to the plan; leaving the plan turns back to the card, which
/// still answers.
#[test]
fn reading_turns_to_the_plan_and_leaving_it_turns_back_to_the_card() {
    let review = review();
    let mut deciding = deciding(&review, Opens::TheCard);
    assert!(!shows_the_plan(&mut deciding));
    assert_eq!(deciding.press(Stroke::Insert('r')), Press::Moved);
    assert!(shows_the_plan(&mut deciding));
    assert_eq!(deciding.press(Stroke::Decline), Press::Moved);
    assert!(!shows_the_plan(&mut deciding));
    assert!(matches!(deciding.press(Stroke::Enter), Press::Answer(_)));
}

/// A number confirmed at the desk answers with the card's option at that place.
#[test]
fn a_number_picked_at_the_plan_answers_with_the_cards_option() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    assert_eq!(deciding.press(Stroke::Insert('1')), Press::Stay);
    assert_eq!(
        deciding.press(Stroke::Enter),
        Press::Answer(PickedOption {
            option: OptionId::from_static("approve"),
            words: None,
            folded: false,
        })
    );
}

/// An option that asks for words, picked at the desk, opens its field on the card.
#[test]
fn a_number_whose_option_asks_for_words_opens_its_field_on_the_card() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    assert_eq!(deciding.press(Stroke::Insert('2')), Press::Stay);
    assert_eq!(deciding.press(Stroke::Enter), Press::Moved);
    assert!(!shows_the_plan(&mut deciding));
    assert!(deciding.typing(), "the field of `adjust` is open");
}

/// Ctrl-C at the desk stops the run, as it does at the card.
#[test]
fn ctrl_c_at_the_plan_stops_the_run() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    assert_eq!(deciding.press(Stroke::Interrupt), Press::Interrupt);
}

/// Where the plan has no room, the card stands in front.
#[test]
fn a_plan_with_no_room_leaves_the_card_in_front() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    deciding.plan_unshown();
    assert!(!shows_the_plan(&mut deciding));
}

/// A card with no plan behind it does not offer one: `r` stays on the card.
#[test]
fn a_decision_with_no_plan_holds_only_its_card() {
    let mut deciding = Deciding::new(card());
    deciding.press(Stroke::Insert('r'));
    assert!(!shows_the_plan(&mut deciding));
}

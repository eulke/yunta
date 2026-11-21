//! How a decision turns between its card and the plan behind it, with no terminal: a
//! stroke goes in, and what it came to and the one in front come out.

use surco::doc::Block;
use surco::surface::Terminal;
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
        workflow: Some("harvest".to_string()),
    };
    Deciding::on_plan(card(), Reading::of(review.clone()), asked, opens)
}

fn shows_the_plan(deciding: &mut Deciding) -> bool {
    let block_is_the_desk = matches!(deciding.block(&look()), Block::Desk(_));
    let held_is_the_desk = matches!(deciding.in_front(), InFront::ThePlan(_));
    assert_eq!(block_is_the_desk, held_is_the_desk, "drawn and held agree");
    held_is_the_desk
}

/// The one in front as a terminal as wide as the desk's tests' look holds it, 30 rows high:
/// each row's words.
fn rows(deciding: &Deciding) -> Vec<String> {
    let look = look();
    let held = Terminal::on(look).held(&deciding.block(&look), look.width.cells(), 30);
    held.lines.iter().map(|line| line.text()).collect()
}

fn holds(deciding: &Deciding, needle: &str) -> bool {
    rows(deciding).iter().any(|row| row.contains(needle))
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

/// A number confirmed at the desk answers with the card's option at that place, the desk
/// staying in front of what was answered there.
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
    assert!(shows_the_plan(&mut deciding), "the desk stays in front");
}

/// An option that asks for words, confirmed at the desk, opens its line at the desk's foot,
/// the desk staying in front: the words typed there answer with it.
#[test]
fn a_number_whose_option_asks_for_words_opens_its_line_at_the_desk() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    assert_eq!(deciding.press(Stroke::Insert('2')), Press::Stay);
    assert_eq!(deciding.press(Stroke::Enter), Press::Stay);
    assert!(shows_the_plan(&mut deciding), "the desk stays in front");
    assert!(deciding.typing(), "the line of `adjust` is open");
    for character in "keep the lock".chars() {
        assert_eq!(deciding.press(Stroke::Insert(character)), Press::Stay);
    }
    assert!(
        shows_the_plan(&mut deciding),
        "typing keeps the desk in front"
    );
    assert_eq!(
        deciding.press(Stroke::Enter),
        Press::Answer(PickedOption {
            option: OptionId::from_static("adjust"),
            words: Some("keep the lock".to_string()),
            folded: false,
        })
    );
}

/// The line at the desk's foot stands in place of the options, under the row of the option
/// it is for, what that option asks faint in it while nothing is typed. An empty line is
/// refused under it, and `esc` goes back to the options with nothing kept.
#[test]
fn the_line_at_the_desks_foot_refuses_no_words_and_esc_goes_back_to_the_options() {
    let review = review();
    let mut deciding = deciding(&review, Opens::ThePlan);
    deciding.press(Stroke::Insert('2'));
    deciding.press(Stroke::Enter);
    let foot = rows(&deciding);
    let at = foot
        .iter()
        .position(|row| row.contains("adjust  Back to `plan`"))
        .unwrap_or_else(|| panic!("the option's row stands: {foot:#?}"));
    assert!(foot[at + 2].contains("│ what should change?"), "{foot:#?}");
    assert!(!holds(&deciding, "1 approve"), "{foot:#?}");
    assert_eq!(deciding.press(Stroke::Enter), Press::Stay, "nothing typed");
    assert!(holds(&deciding, "✗ this option needs an answer"));
    deciding.press(Stroke::Insert('x'));
    assert_eq!(deciding.press(Stroke::Decline), Press::Stay);
    assert!(shows_the_plan(&mut deciding) && !deciding.typing());
    assert!(holds(&deciding, "1 approve   2 adjust"));
    deciding.press(Stroke::Insert('2'));
    deciding.press(Stroke::Enter);
    assert!(holds(&deciding, "│ what should change?"), "nothing kept");
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

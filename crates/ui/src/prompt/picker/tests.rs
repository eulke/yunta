//! What the one list does with each key, with no terminal.

use surco::look::Look;
use yunta_render::prompt::Escape;

use super::{Alternative, Picker};
use crate::prompt::{Answerable, Press, Stroke};

/// A terminal with room for two rows of a list, under the three a held list leaves to
/// its keys, the question above it and the terminal's last row, which the cursor is
/// left on.
const TWO_ROWS: usize = 5;

fn picker() -> Picker<&'static str> {
    Picker::new(
        vec![
            Alternative::named("staging", "staging"),
            Alternative::named("production", "production"),
            Alternative::named("preview", "preview"),
        ],
        Escape::Parks,
    )
}

fn rows(picker: &Picker<&'static str>, most: usize) -> Vec<String> {
    held(picker, most)
        .lines
        .iter()
        .map(|line| line.text())
        .collect()
}

#[test]
fn enter_takes_the_row_pointed_at_and_arrows_move_it() {
    let mut list = picker();
    assert_eq!(list.press(Stroke::Down), Press::Stay);
    assert_eq!(list.press(Stroke::Enter), Press::Answer("production"));
}

/// Typing narrows the list to what it matches, in order, and the pointer stays on
/// a row the list still holds.
#[test]
fn typing_narrows_the_list_and_enter_takes_the_first_it_leaves() {
    let mut list = picker();
    list.press(Stroke::Down);
    list.press(Stroke::Down);
    for key in ['p', 'r', 'o'] {
        assert_eq!(list.press(Stroke::Insert(key)), Press::Stay);
    }
    let drawn = rows(&list, usize::MAX);
    assert!(
        drawn.iter().any(|row| row.contains("* production")),
        "{drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.contains("preview")),
        "{drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.contains("staging")),
        "{drawn:?}"
    );
    assert_eq!(list.press(Stroke::Enter), Press::Answer("production"));
}

/// A filter matches letters in order, whatever their case and whatever stands
/// between them.
#[test]
fn a_filter_matches_letters_in_order_whatever_their_case() {
    let mut list = picker();
    for key in ['P', 'V'] {
        list.press(Stroke::Insert(key));
    }
    assert_eq!(list.press(Stroke::Enter), Press::Answer("preview"));
}

/// A list has no number per row: a digit narrows it as any letter does, so a value
/// with digits in it is typed as written.
#[test]
fn a_digit_narrows_the_list_as_any_letter_does() {
    let mut list = Picker::new(
        vec![
            Alternative::named("v1", "v1"),
            Alternative::named("v2", "v2"),
        ],
        Escape::Parks,
    );
    list.press(Stroke::Insert('2'));
    assert_eq!(list.press(Stroke::Enter), Press::Answer("v2"));
}

/// The pointer goes around at the ends of a list, as it does on a card.
#[test]
fn the_pointer_goes_around_at_the_ends() {
    let mut list = picker();
    assert_eq!(list.press(Stroke::Up), Press::Stay);
    assert_eq!(list.press(Stroke::Enter), Press::Answer("preview"));
    let mut list = picker();
    for _ in 0..3 {
        list.press(Stroke::Down);
    }
    assert_eq!(list.press(Stroke::Enter), Press::Answer("staging"));
}

/// What answers what was typed best stands first: all of it, then its start, then a
/// run of it, then its letters in order, and in the order given among equals.
#[test]
fn the_best_answer_to_what_was_typed_stands_first() {
    let picked = |values: Vec<&'static str>, typed: &str| {
        let mut list = Picker::new(
            values
                .into_iter()
                .map(|value| Alternative::named(value, value))
                .collect(),
            Escape::Parks,
        );
        for key in typed.chars() {
            list.press(Stroke::Insert(key));
        }
        list.press(Stroke::Enter)
    };
    assert_eq!(
        picked(vec!["reshape", "shape"], "shape"),
        Press::Answer("shape")
    );
    assert_eq!(
        picked(vec!["reshape", "shapes"], "shape"),
        Press::Answer("shapes")
    );
    assert_eq!(
        picked(vec!["s-h-a-p-e", "reshape"], "shape"),
        Press::Answer("reshape")
    );
    assert_eq!(
        picked(vec!["shape-a", "shape-b"], "shape"),
        Press::Answer("shape-a")
    );
}

#[test]
fn backspace_widens_the_list_again() {
    let mut list = picker();
    list.press(Stroke::Insert('z'));
    assert!(!rows(&list, usize::MAX)
        .iter()
        .any(|row| row.contains("staging")));
    list.press(Stroke::Backspace);
    assert!(rows(&list, usize::MAX)
        .iter()
        .any(|row| row.contains("staging")));
}

/// A filter that leaves nothing answers nothing: enter stays until it matches.
#[test]
fn enter_on_an_empty_list_answers_nothing() {
    let mut list = picker();
    list.press(Stroke::Insert('z'));
    assert_eq!(list.press(Stroke::Enter), Press::Stay);
}

#[test]
fn esc_declines_and_ctrl_c_interrupts() {
    assert_eq!(picker().press(Stroke::Decline), Press::Decline);
    assert_eq!(picker().press(Stroke::Interrupt), Press::Interrupt);
}

/// The list says its keys, and what esc does here.
#[test]
fn the_list_says_its_keys() {
    let drawn = rows(&picker(), usize::MAX);
    let keys = drawn.last().cloned().unwrap_or_default();
    assert!(keys.contains("type to filter"), "{drawn:?}");
    assert!(keys.contains("esc leaves it for later"), "{drawn:?}");
}

/// A list too long for its room shows the rows around the pointer.
#[test]
fn a_long_list_keeps_the_pointer_in_its_room() {
    let mut list = picker();
    list.press(Stroke::Down);
    list.press(Stroke::Down);
    let drawn = rows(&list, TWO_ROWS);
    assert!(
        drawn.iter().any(|row| row.contains("* preview")),
        "{drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.contains("staging")),
        "{drawn:?}"
    );
}

/// `prompt` as a terminal of `rows` rows holds it, at the measure of a line.
fn held(prompt: &impl Answerable, rows: usize) -> surco::surface::Placed {
    let look = Look::plain();
    surco::surface::Terminal::on(look).held(&prompt.block(&look), look.width.cells(), rows)
}

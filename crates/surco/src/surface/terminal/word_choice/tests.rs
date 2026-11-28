//! A row of words with one chosen, at a terminal: the pointer marks the chosen word on every
//! layer, and a row too narrow for every word keeps the chosen one in sight. Every expected
//! row is written by hand; those of the screens are their targets' rows, from the region's
//! first cell.

use crate::components::WordChoice;
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::Terminal;

/// `words` in a row, the one at `chosen` chosen, the keys acting on it or not.
fn row(words: &[&str], chosen: usize, focused: bool) -> WordChoice {
    WordChoice {
        words: words.iter().map(|word| word.to_string()).collect(),
        chosen,
        focused,
    }
}

/// The row drawn `cells` wide in `glyphs`, unpainted.
fn drawn(choice: &WordChoice, cells: usize, glyphs: Glyphs) -> Line {
    let look = Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: crate::motion::Motion::Still,
    };
    Terminal::on(look).word_choice(choice).unwrap_or_default()
}

/// The tone each word of `line` stands in, its pointer and its ellipses among them.
fn tones(line: &Line) -> Vec<(String, Tone)> {
    line.spans()
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .map(|span| (span.text.clone(), span.tone))
        .collect()
}

const SIX: [&str; 6] = ["one", "two", "three", "four", "five", "six"];

/// The pointer stands in the cell two before the chosen word, in Unicode and in ASCII alike,
/// so a layer with no tones still says which word is chosen; the chosen word is strong, the
/// rest muted, and the pointer takes the tone of a pointer a person moves.
#[test]
fn the_chosen_word_carries_the_pointer_in_unicode_and_in_ascii() {
    let modes = row(&["quick", "standard", "full"], 1, true);
    assert_eq!(
        drawn(&modes, 60, Glyphs::Unicode).text(),
        "   quick ▸ standard   full"
    );
    assert_eq!(
        drawn(&modes, 60, Glyphs::Ascii).text(),
        "   quick * standard   full"
    );
    assert_eq!(
        tones(&drawn(&modes, 60, Glyphs::Unicode)),
        [
            ("quick".to_string(), Tone::Muted),
            ("▸".to_string(), Tone::NeedsYou),
            ("standard".to_string(), Tone::Strong),
            ("full".to_string(), Tone::Muted),
        ]
    );
}

/// Where the keys act on another row, the pointer is muted: it still marks the chosen word,
/// which stays strong.
#[test]
fn a_row_the_keys_do_not_act_on_keeps_its_pointer_muted() {
    let modes = row(&["quick", "standard", "full"], 0, false);
    assert_eq!(
        tones(&drawn(&modes, 60, Glyphs::Unicode)),
        [
            ("▸".to_string(), Tone::Muted),
            ("quick".to_string(), Tone::Strong),
            ("standard".to_string(), Tone::Muted),
            ("full".to_string(), Tone::Muted),
        ]
    );
}

/// At 30 cells a row of six words keeps the fifth in sight when it is chosen: the words before
/// it give way from the first, an ellipsis standing where they would, and the sixth still fits.
#[test]
fn at_thirty_cells_a_row_of_six_words_keeps_the_fifth_in_sight_when_it_is_chosen() {
    let fifth = row(&SIX, 4, true);
    assert_eq!(
        drawn(&fifth, 30, Glyphs::Unicode).text(),
        "…  three   four ▸ five   six"
    );
    assert_eq!(
        drawn(&fifth, 30, Glyphs::Ascii).text(),
        "...  three   four * five   six"
    );
}

/// Where the words after the chosen one are lost, the ellipsis stands after the last in sight,
/// where the next would start, and the row keeps its first words; where words are lost on
/// both sides, an ellipsis stands on each, the row keeping as many words before the chosen one
/// as fit.
#[test]
fn the_ellipsis_stands_on_the_side_that_lost_words() {
    assert_eq!(
        drawn(&row(&SIX, 1, true), 30, Glyphs::Unicode).text(),
        "   one ▸ two   three   four  …"
    );
    assert_eq!(
        drawn(&row(&SIX, 3, true), 24, Glyphs::Unicode).text(),
        "…  two   three ▸ four  …"
    );
}

/// A chosen word wider than the row alone is cut at the row's end, its pointer kept.
#[test]
fn a_chosen_word_wider_than_the_row_is_cut_at_its_end() {
    let wide = row(&["as the runners in config.yaml say", "codex"], 0, true);
    assert_eq!(
        drawn(&wide, 20, Glyphs::Unicode).text(),
        " ▸ as the runners i…"
    );
}

/// The rows of the screens that hold a row of words, as their targets draw them: a new run's
/// mode and adapter, at 120 columns and in the 58 cells of 60, and a role's adapters changed
/// in a setup.
#[test]
fn the_rows_are_the_targets_rows() {
    let modes = row(&["quick", "standard", "full"], 0, false);
    assert_eq!(
        drawn(&modes, 71, Glyphs::Unicode).text(),
        " ▸ quick   standard   full"
    );
    let adapters = row(
        &["as the runners in config.yaml say", "claude-code", "codex"],
        0,
        false,
    );
    let wide = " ▸ as the runners in config.yaml say   claude-code   codex";
    assert_eq!(drawn(&adapters, 71, Glyphs::Unicode).text(), wide);
    assert_eq!(drawn(&adapters, 58, Glyphs::Unicode).text(), wide);
    assert_eq!(
        drawn(&adapters, 71, Glyphs::Ascii).text(),
        " * as the runners in config.yaml say   claude-code   codex"
    );
    let roles = row(&["claude-code", "codex"], 0, true);
    assert_eq!(
        drawn(&roles, 40, Glyphs::Unicode).text(),
        " ▸ claude-code   codex"
    );
}

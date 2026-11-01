#[test]
fn every_glyph_a_diagram_is_drawn_with_is_ascii_in_the_ascii_set() {
    let ways = [false, true];
    let strokes = [Stroke::Arrow, Stroke::Open, Stroke::Dotted, Stroke::Thick];
    let mut drawn = Vec::new();
    for up in ways {
        for down in ways {
            for left in ways {
                for right in ways {
                    for stroke in strokes {
                        drawn.push(Glyphs::Ascii.joint(up, down, left, right, stroke));
                    }
                }
            }
        }
    }
    for shape in [Shape::Box, Shape::Round, Shape::Decision] {
        drawn.extend(Glyphs::Ascii.corners(shape));
    }
    drawn.extend([Glyphs::Ascii.head(true), Glyphs::Ascii.head(false)]);
    drawn.extend(Glyphs::Ascii.joins().chars());
    drawn.push(Glyphs::Ascii.continued());
    assert!(drawn.iter().all(char::is_ascii), "{drawn:?}");
}

use super::*;

fn env(explicit: Option<&str>, locale: Option<&str>, term: Option<&str>) -> GlyphEnv {
    GlyphEnv {
        explicit: explicit.map(str::to_string),
        locale: locale.map(str::to_string),
        term: term.map(str::to_string),
    }
}

#[test]
fn an_explicit_choice_wins_over_both_other_signals() {
    let ascii_environment = env(Some("unicode"), Some("C"), Some("dumb"));
    assert_eq!(Glyphs::select(&ascii_environment), Glyphs::Unicode);
    let unicode_environment = env(Some("ASCII "), Some("en_US.UTF-8"), Some("xterm"));
    assert_eq!(Glyphs::select(&unicode_environment), Glyphs::Ascii);
}

#[test]
fn a_locale_that_names_no_utf8_gets_ascii() {
    assert_eq!(
        Glyphs::select(&env(None, Some("C"), Some("xterm"))),
        Glyphs::Ascii
    );
    assert_eq!(
        Glyphs::select(&env(None, None, Some("xterm"))),
        Glyphs::Ascii
    );
    assert_eq!(
        Glyphs::select(&env(None, Some("ja_JP.UTF-8"), Some("xterm"))),
        Glyphs::Unicode
    );
}

#[test]
fn a_dumb_terminal_gets_ascii_even_under_a_utf8_locale() {
    assert_eq!(
        Glyphs::select(&env(None, Some("en_US.utf8"), Some("dumb"))),
        Glyphs::Ascii
    );
}

#[test]
fn an_unrecognized_override_leaves_the_environment_to_decide() {
    assert_eq!(
        Glyphs::select(&env(Some("fancy"), Some("en_US.UTF-8"), None)),
        Glyphs::Unicode
    );
    assert_eq!(
        Glyphs::select(&env(Some("fancy"), Some("C"), None)),
        Glyphs::Ascii
    );
}

#[test]
fn every_glyph_of_either_set_occupies_exactly_one_cell() {
    for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
        let mut drawn: Vec<char> = vec![glyphs.bar_filled(), glyphs.bar_empty()];
        drawn.extend(glyphs.ramp());
        drawn.extend(ALL_MARKS.map(|mark| glyphs.mark(mark)));
        for ch in drawn {
            assert_eq!(
                crate::width::cell_width(&ch.to_string()),
                1,
                "{ch:?} is not one cell wide under {glyphs}"
            );
        }
    }
}

/// What a line of text is marked and joined with in one set: the marks,
/// and every sign a sentence or a row carries.
fn tokens(glyphs: Glyphs) -> Vec<String> {
    let mut tokens: Vec<String> = ALL_MARKS
        .iter()
        .map(|mark| glyphs.mark(*mark).to_string())
        .collect();
    tokens.extend(
        [
            glyphs.sep(),
            glyphs.gutter(),
            glyphs.times(),
            glyphs.bar_filled(),
            glyphs.bar_empty(),
            glyphs.continued(),
            HOME,
        ]
        .map(String::from),
    );
    tokens.push(glyphs.ellipsis().to_string());
    tokens.push(glyphs.within().to_string());
    tokens
}

#[test]
fn no_mark_or_sign_of_a_set_repeats_another() {
    for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
        let tokens = tokens(glyphs);
        let mut seen = std::collections::BTreeSet::new();
        for token in &tokens {
            assert!(
                seen.insert(token.clone()),
                "{glyphs}: `{token}` says two things"
            );
        }
    }
}

#[test]
fn the_ascii_set_is_ascii() {
    for token in tokens(Glyphs::Ascii) {
        assert!(token.is_ascii(), "`{token}`");
    }
}

/// The marks of a word diff are a family of their own, kept out of
/// `tokens`: they never stand beside the words of a line, only inside a
/// line of code.
#[test]
fn the_marks_of_a_word_diff_are_ascii_and_apart_in_both_sets() {
    for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
        let marks: Vec<&str> = glyphs.added().into_iter().chain(glyphs.removed()).collect();
        let apart: std::collections::BTreeSet<&str> = marks.iter().copied().collect();
        assert_eq!(apart.len(), 4, "{glyphs}: {marks:?}");
        assert!(
            marks.iter().all(|mark| mark.is_ascii()),
            "{glyphs}: {marks:?}"
        );
        let tokens = tokens(glyphs);
        for mark in &marks {
            assert!(
                !tokens.iter().any(|token| token == mark),
                "{glyphs}: `{mark}` says what a sign of the set says"
            );
        }
    }
}

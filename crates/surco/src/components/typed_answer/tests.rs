//! A typed answer is a block every surface draws: as Markdown, as a page, and on a
//! terminal in every environment.

use std::path::{Path, PathBuf};

use super::TypedAnswer;
use crate::components::TypedRow;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/typed_answer")
}

/// The words typed before the cursor and after it, opened as an answer is.
fn typed(before: &str, after: &str) -> TypedRow {
    TypedRow {
        prompt: "> ".to_string(),
        before: before.to_string(),
        after: after.to_string(),
    }
}

/// A project's name asked, its detected value standing, and a name half typed over it.
pub(crate) fn named() -> TypedAnswer {
    TypedAnswer {
        asked: Line::new()
            .push(Tone::Strong, "project name")
            .push(Tone::Muted, " (enter keeps `yunta`)"),
        typed: typed("yunta-", "tui"),
        placeholder: None,
        hint: None,
    }
}

/// What a person wants built, asked with nothing typed yet: what such a request reads like
/// stands in the box, and what becomes of it under the box.
pub(crate) fn requested() -> TypedAnswer {
    TypedAnswer {
        asked: Line::new().push(Tone::Strong, "What do you want to build?"),
        typed: typed("", ""),
        placeholder: Some("add support for installing a pack globally".to_string()),
        hint: Some("this becomes the brief; fragua asks before it decides for you".to_string()),
    }
}

/// A plain Unicode look exactly `cells` wide.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: crate::motion::Motion::Still,
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// The tones `line` says something in, in order, once for each run of spans in one tone.
fn tones(line: &Line) -> Vec<Tone> {
    let mut tones: Vec<Tone> = line
        .spans()
        .iter()
        .filter(|span| !span.text.trim().is_empty())
        .map(|span| span.tone)
        .collect();
    tones.dedup();
    tones
}

#[test]
fn a_typed_answer_matches_its_goldens_on_every_surface() {
    for (case, answer) in [("named", named()), ("requested", requested())] {
        let doc = Doc::new().with(answer);
        assert_golden(&goldens().join(format!("{case}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{case}.html")), &Html.body(&doc));
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}

/// Held, the cursor stands after what was typed before it, inside the box under the
/// question: on its middle row, past its upright and the cell inside it.
#[test]
fn a_held_answer_puts_the_cursor_where_the_typing_is() {
    let look = Look::of(&ENVIRONMENTS[0]);
    let held = named().drawn(&look);
    assert_eq!(held.cursor, Some((2, "│ yunta-".chars().count())));
}

/// While nothing is typed, the placeholder stands faint where the words go and the cursor
/// on the first cell inside the box, where the first word typed lands; once a word is typed,
/// the word stands there in its place.
#[test]
fn the_placeholder_shows_faint_only_while_nothing_is_typed() {
    let empty = requested().drawn(&look(40));
    assert_eq!(
        empty.lines[2].text(),
        "│ add support for installing a pack g… │"
    );
    assert_eq!(
        tones(&empty.lines[2]),
        [Tone::Muted, Tone::Faint, Tone::Muted]
    );
    assert_eq!(empty.cursor, Some((2, "│ ".chars().count())));

    let typing = TypedAnswer {
        typed: typed("a", ""),
        ..requested()
    }
    .drawn(&look(40));
    assert_eq!(typing.lines[2].text(), format!("│ a{} │", " ".repeat(35)));
    assert_eq!(
        tones(&typing.lines[2]),
        [Tone::Muted, Tone::Strong, Tone::Muted]
    );
    assert_eq!(typing.cursor, Some((2, "│ a".chars().count())));
}

/// The hint stands under the box, muted, going on under itself where it is wider than the
/// look; an answer with no hint ends with the box.
#[test]
fn a_hint_stands_muted_under_the_box_and_none_where_there_is_none() {
    let hinted = requested().drawn(&look(40));
    assert_eq!(
        texts(&hinted.lines[3..]),
        [
            "╰──────────────────────────────────────╯",
            "this becomes the brief; fragua asks",
            "before it decides for you",
        ]
    );
    for row in &hinted.lines[4..] {
        assert_eq!(tones(row), [Tone::Muted], "{:?}", row.spans());
    }

    let bare = TypedAnswer {
        hint: None,
        ..requested()
    }
    .drawn(&look(40));
    assert_eq!(bare.lines.len(), 4, "{:#?}", texts(&bare.lines));
    assert_eq!(
        bare.lines[3].text(),
        "╰──────────────────────────────────────╯"
    );
}

/// At the first request's width the question and its box are the design's rows, and at a
/// new run's the hint stands under them as the design draws it there.
#[test]
fn the_request_draws_the_designs_rows_for_the_same_words_at_the_same_width() {
    let first_request = TypedAnswer {
        hint: None,
        ..requested()
    };
    assert_eq!(
        texts(&first_request.drawn(&look(72)).lines),
        [
            "What do you want to build?",
            "╭──────────────────────────────────────────────────────────────────────╮",
            "│ add support for installing a pack globally                           │",
            "╰──────────────────────────────────────────────────────────────────────╯",
        ]
    );
    assert_eq!(
        texts(&requested().drawn(&look(71)).lines),
        [
            "What do you want to build?",
            "╭─────────────────────────────────────────────────────────────────────╮",
            "│ add support for installing a pack globally                          │",
            "╰─────────────────────────────────────────────────────────────────────╯",
            "this becomes the brief; fragua asks before it decides for you",
        ]
    );
}

//! A document in fewer rows than it takes, on a terminal: a page of it from the line asked,
//! the row that counts what is below, its last lines, the line a pointer stands on kept in
//! sight, and what was laid out of it.

use crate::components::{Laid, Sight};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::terminal::Terminal;

/// A terminal 40 cells wide, in Unicode glyphs, unpainted.
fn terminal() -> Terminal {
    Terminal::on(Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(40),
        motion: Motion::Moves,
    })
}

/// A document of `count` lines, `line 0` first, the line at `pointed` led by the pointer.
fn numbered(count: usize, pointed: Option<usize>) -> Doc {
    let pointer = Glyphs::Unicode.pointer();
    let lines = (0..count)
        .map(|at| match pointed == Some(at) {
            true => Line::new().plain(format!("{pointer} line {at}")),
            false => Line::new().plain(format!("line {at}")),
        })
        .collect();
    Doc::new().with(Block::Lines(lines))
}

/// Each row's text.
fn texts(rows: &[Line]) -> Vec<String> {
    rows.iter().map(Line::text).collect()
}

#[test]
fn a_page_shows_from_the_line_asked_and_counts_what_is_below() {
    let (rows, laid) = terminal().scrolled(&numbered(20, None), Sight::from_line(3), 5);
    assert_eq!(
        texts(&rows),
        ["line 3", "line 4", "line 5", "line 6", "… 13 more lines"]
    );
    assert_eq!(
        laid,
        Laid {
            top: 3,
            page: 5,
            most: 15
        }
    );
    assert!(laid.scrolls());
}

#[test]
fn past_the_last_first_line_the_last_lines_show_with_no_count() {
    let (rows, laid) = terminal().scrolled(&numbered(20, None), Sight::at_the_end(), 5);
    assert_eq!(
        texts(&rows),
        ["line 15", "line 16", "line 17", "line 18", "line 19"]
    );
    assert_eq!(laid.top, 15);
}

#[test]
fn the_line_a_pointer_stands_on_is_kept_in_sight() {
    let (rows, laid) = terminal().scrolled(&numbered(20, Some(12)), Sight::following_pointer(), 5);
    assert_eq!(laid.top, 12);
    let pointed = format!("{} line 12", Glyphs::Unicode.pointer());
    assert_eq!(rows.first().map(Line::text), Some(pointed));
    // Where the pointer is in sight already, the line asked stays.
    let asked = Sight {
        top: 10,
        follows_pointer: true,
    };
    let (_, laid) = terminal().scrolled(&numbered(20, Some(12)), asked, 5);
    assert_eq!(laid.top, 10);
}

#[test]
fn a_document_that_fits_shows_whole_and_does_not_scroll() {
    let (rows, laid) = terminal().scrolled(&numbered(3, None), Sight::from_line(2), 5);
    assert_eq!(texts(&rows), ["line 0", "line 1", "line 2"]);
    assert_eq!(
        laid,
        Laid {
            top: 0,
            page: 5,
            most: 0
        }
    );
    assert!(!laid.scrolls());
}

#[test]
fn the_last_lines_show_and_move_up_to_a_pointer_above_them() {
    let at_the_question = Sight::at_the_end_with_the_pointer();
    let (rows, _) = terminal().scrolled(&numbered(20, None), at_the_question, 5);
    assert_eq!(rows.last().map(Line::text).as_deref(), Some("line 19"));
    let (rows, laid) = terminal().scrolled(&numbered(20, Some(3)), at_the_question, 5);
    assert_eq!(laid.top, 3);
    let pointed = format!("{} line 3", Glyphs::Unicode.pointer());
    assert_eq!(rows.first().map(Line::text), Some(pointed));
}

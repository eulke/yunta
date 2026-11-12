//! A region in a pane: a prompt held in the rows what stands around it leaves, with the
//! cursor counted from the pane's top, what stands above it giving way from its top and
//! what stands under it kept whole; any other region scrolled, as a document scrolls.

use std::path::{Path, PathBuf};

use crate::components::{
    Card, CardBody, Choices, Desk, Note, Prose, Region, Sight, TypedAnswer, TypedRow,
};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::terminal::card::tests::{card, eight_rows};
use crate::surface::terminal::desk::tests::{desk, numbered};
use crate::surface::Terminal;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/pane")
}

/// The look of a pane as wide as `environment`'s lines: a pane's width is exact.
fn pane_look(environment: &Environment) -> Look {
    Look {
        width: Width::exact(environment.width),
        ..Look::of(environment)
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// A page of `count` lines, `line 1` to `line <count>`.
fn page(count: usize) -> Doc {
    let lines: Vec<Line> = (1..=count)
        .map(|at| Line::new().plain(format!("line {at}")))
        .collect();
    Doc::new().with(lines)
}

/// A project's name asked, its detected value standing, and a name half typed over it.
fn named() -> TypedAnswer {
    TypedAnswer {
        asked: Line::new()
            .push(Tone::Strong, "project name")
            .push(Tone::Muted, " (enter keeps `yunta`)"),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "yunta-".to_string(),
            after: "tui".to_string(),
        },
    }
}

/// A decision of eight options, the third pointed at.
fn choosing(glyphs: Glyphs) -> Card {
    let body = CardBody::Choosing {
        choices: Choices {
            rows: eight_rows(),
            at: 2,
        },
        then: None,
    };
    card(body, glyphs)
}

/// A plan's desk of thirty lines to read.
fn plan() -> Desk {
    desk(numbered(30))
}

/// What a project is called, asked under the sentence that says what the name is for.
fn named_under_a_sentence() -> Region {
    Region::new(
        Doc::new()
            .with(Prose("The name the project goes by.".to_string()))
            .with(named()),
    )
}

#[test]
fn a_prompt_is_held_under_what_stands_above_it_with_the_cursor_counted_from_the_top() {
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&named_under_a_sentence(), 6);
    assert_eq!(
        texts(&paned.lines),
        [
            "  The name the project goes by.",
            "",
            "project name (enter keeps `yunta`)",
            "> yunta-tui",
        ]
    );
    assert_eq!(paned.cursor, Some((3, "> yunta-".len())));
}

/// What stands under a prompt, as a note of what was refused, stays whole under it.
#[test]
fn what_stands_under_a_prompt_stays_whole_under_it() {
    let region = Region::new(
        Doc::new()
            .with(named())
            .with(Note("a project is already called that".to_string())),
    );
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&region, 6);
    assert_eq!(
        texts(&paned.lines),
        [
            "project name (enter keeps `yunta`)",
            "> yunta-tui",
            "",
            "  a project is already called that",
        ]
    );
    assert_eq!(paned.cursor, Some((1, "> yunta-".len())));
}

/// Short of rows, what stands above a prompt keeps at most half the pane, from the line
/// nearest the prompt up: the prompt is what a person answers, and its question stays.
#[test]
fn what_stands_above_a_prompt_gives_way_from_its_top() {
    let region = Region::new(
        Doc::new()
            .with_all(page(10).into_blocks())
            .with(choosing(Glyphs::Ascii)),
    );
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&region, 10);
    let text = texts(&paned.lines);
    assert!(paned.lines.len() <= 10, "{text:#?}");
    assert_eq!(text[0], "line 7", "{text:#?}");
    assert_eq!(text[3], "line 10", "{text:#?}");
    assert!(
        text.iter()
            .any(|line| line.contains("Plan and its tests registered. Approve?")),
        "{text:#?}"
    );
}

/// A pane is no terminal's bottom: a prompt in one fills every row it is given, where
/// held where a person is it keeps the terminal's last row free.
#[test]
fn a_prompt_in_a_pane_fills_every_row_it_is_given() {
    let terminal = Terminal::on(pane_look(&ENVIRONMENTS[1]));
    let paned = terminal.pane(&Region::new(Doc::new().with(plan())), 12);
    assert_eq!(paned.lines.len(), 12, "{:#?}", texts(&paned.lines));
    assert!(
        paned.laid.page > 0,
        "what scrolls in the desk is handed back"
    );
    let held = terminal.held(&Block::Desk(plan()), 80, 12);
    assert_eq!(held.lines.len(), 11);
}

/// A region with no prompt scrolls from its sight, as a document scrolls.
#[test]
fn a_region_with_no_prompt_scrolls_from_its_sight() {
    let terminal = Terminal::on(pane_look(&ENVIRONMENTS[1]));
    let region = Region::new(page(20)).in_sight(Sight::from_line(3));
    let paned = terminal.pane(&region, 5);
    let (lines, laid) = terminal.scrolled(&page(20), Sight::from_line(3), 5);
    assert_eq!(texts(&paned.lines), texts(&lines));
    assert_eq!(paned.laid, laid);
    assert_eq!(paned.cursor, None);
}

/// A card under a sentence held in a short pane, a plan's desk in a pane of its own, and a
/// page scrolled past its first lines, each with the rows its pane has.
fn cases(glyphs: Glyphs) -> [(&'static str, Region, usize); 3] {
    let card = Doc::new()
        .with(Prose("The plan the planner handed over.".to_string()))
        .with(choosing(glyphs));
    [
        ("held-card", Region::new(card), 10),
        ("held-desk", Region::new(Doc::new().with(plan())), 14),
        (
            "scrolled-page",
            Region::new(page(30)).in_sight(Sight::from_line(5)),
            8,
        ),
    ]
}

#[test]
fn a_pane_matches_its_goldens_in_every_environment() {
    for environment in &ENVIRONMENTS {
        let look = pane_look(environment);
        let terminal = Terminal::on(look);
        for (case, region, rows) in cases(look.glyphs) {
            let paned = terminal.pane(&region, rows);
            let drawn: String = paned
                .lines
                .iter()
                .map(|line| format!("{}\n", terminal.look.paint.line(line)))
                .collect();
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}

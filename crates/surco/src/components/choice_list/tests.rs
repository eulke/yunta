//! A choice list is a block every surface draws: as Markdown, as a page, and on a
//! terminal in every environment; and held, it gives way to the rows it has.

use std::path::{Path, PathBuf};

use super::ChoiceList;
use crate::components::{ChoiceRow, Choices, Legend};
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/choice_list")
}

/// The shapes a new workflow starts as, narrowed by what was typed to the two that
/// hold it, the second pointed at.
pub(crate) fn shapes() -> ChoiceList {
    let shape = |name: &str, said: &str| ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: None,
        key: None,
    };
    ChoiceList {
        choices: Some(Choices {
            rows: vec![
                shape("one-node", "one command whose exit code is the check"),
                shape("lint-fix", "a check, and an agent that fixes what it finds"),
            ],
            at: 1,
        }),
        filter: Some("n".to_string()),
        legend: Legend::new()
            .arrows("choose")
            .key("enter", "answer")
            .key("esc", "take the first"),
    }
}

#[test]
fn a_choice_list_matches_its_goldens_on_every_surface() {
    let doc = Doc::new().with(shapes());
    assert_golden(&goldens().join("shapes.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("shapes.html"), &Html.body(&doc));
    for environment in ENVIRONMENTS.iter().chain(&RICH) {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), "shapes"), &drawn);
    }
}

/// A list a filter leaves empty says so, on every surface.
#[test]
fn a_list_the_filter_empties_says_nothing_matches() {
    let empty = ChoiceList {
        choices: None,
        ..shapes()
    };
    let doc = Doc::new().with(empty);
    assert!(Markdown.draw(&doc).contains("nothing matches"));
    let look = Look::of(&ENVIRONMENTS[0]);
    let drawn = Terminal::on(look).draw(&doc);
    assert!(drawn.contains("nothing matches"), "{drawn}");
}

/// Held in fewer rows than the least of the list beside its keys, a list keeps to its rows:
/// its keys give way, and the row pointed at stays.
#[test]
fn a_list_short_of_rows_keeps_to_them_and_its_keys_give_way() {
    let look = Look::of(&ENVIRONMENTS[1]);
    for rows in [1, 3] {
        let drawn = shapes().drawn(&look, rows);
        let text: Vec<String> = drawn.lines.iter().map(Line::text).collect();
        assert!(drawn.lines.len() <= rows, "{rows} rows: {text:#?}");
        if rows == 3 {
            assert!(
                text.iter().any(|line| line.contains("lint-fix")),
                "{text:#?}"
            );
        }
    }
}

/// A list of `names` with nothing said beside them, pointed `at` one, its keys standing
/// in a screen's legend.
fn named(names: &[&str], at: usize) -> ChoiceList {
    ChoiceList {
        choices: Some(Choices {
            rows: names
                .iter()
                .map(|name| ChoiceRow {
                    name: (*name).to_string(),
                    said: None,
                    cost: None,
                    key: None,
                })
                .collect(),
            at,
        }),
        filter: None,
        legend: Legend::new(),
    }
}

/// A plain look `cells` wide, as a region of a screen gives it.
fn region(glyphs: Glyphs, cells: usize) -> Look {
    Look {
        glyphs,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Moves,
    }
}

/// `list` held in `rows` rows of a region `cells` wide: each row's text.
fn held(list: &ChoiceList, cells: usize, rows: usize) -> Vec<String> {
    list.drawn(&region(Glyphs::Unicode, cells), rows)
        .lines
        .iter()
        .map(Line::text)
        .collect()
}

/// A list's keys wider than its row fold onto rows of their own as a legend folds, between
/// two keys and never inside one.
#[test]
fn a_lists_keys_wider_than_its_row_fold_between_two_keys() {
    let mut list = named(&["claude-code", "codex"], 0);
    list.legend = Legend::new()
        .arrows("choose")
        .key("enter", "answer")
        .key("esc", "keep the detected default");
    let drawn = held(&list, 40, usize::MAX);
    assert_eq!(
        drawn[drawn.len() - 2..],
        ["↑↓ choose · enter answer", "esc keep the detected default"]
    );
}

/// A list longer than its rows keeps the rows around its pointer in sight, under the row
/// it is narrowed on, and says how many it hides above them and how many under them, each
/// count at the column its names stand in.
#[test]
fn a_list_scrolled_past_its_first_row_says_how_many_stand_above_it() {
    let list = named(&["a", "b", "c", "d", "e", "f", "g", "h", "i"], 4);
    assert_eq!(
        held(&list, 40, 6),
        [
            " ▸ type to filter",
            "  … 2 above",
            "  c",
            "  d",
            "▸ e",
            "  … 4 under",
        ]
    );
}

/// What was typed stands after the pointer on the list's first row, strong; where nothing
/// was, `type to filter` stands there, faint. A blank row parts it from the rows.
#[test]
fn the_first_row_says_what_was_typed_or_that_typing_filters() {
    let mut list = named(&["staging", "production"], 0);
    let rows = list.drawn(&region(Glyphs::Unicode, 40), usize::MAX).lines;
    assert_eq!(rows[0].text(), " ▸ type to filter");
    assert_eq!(
        rows[0].spans().last().map(|span| span.tone),
        Some(Tone::Faint)
    );
    assert_eq!(rows[1].text(), "");
    list.filter = Some("pro".to_string());
    let rows = list.drawn(&region(Glyphs::Ascii, 40), usize::MAX).lines;
    assert_eq!(rows[0].text(), " * pro");
    assert_eq!(
        rows[0].spans().last().map(|span| span.tone),
        Some(Tone::Strong)
    );
}

/// Every printable key types on a list's first row, so the cursor stands where the words
/// land there: at the first cell of `type to filter` while nothing is typed, after the
/// words typed otherwise, whatever the list's width.
#[test]
fn the_cursor_stands_where_typing_lands_on_the_first_row() {
    let mut list = named(&["staging", "production"], 0);
    for cells in [120, 60] {
        let placed = list.drawn(&region(Glyphs::Unicode, cells), usize::MAX);
        assert_eq!(placed.cursor, Some((0, 3)), "{cells} cells, nothing typed");
    }
    list.filter = Some("pro".to_string());
    for cells in [120, 60] {
        let placed = list.drawn(&region(Glyphs::Ascii, cells), usize::MAX);
        assert_eq!(placed.cursor, Some((0, 6)), "{cells} cells, `pro` typed");
    }
    let short = list.drawn(&region(Glyphs::Unicode, 60), 2);
    assert_eq!(short.cursor, Some((0, 6)), "short of rows, the row stays");
}

/// The palette's acts, each by what it does, the first pointed at.
fn acts() -> ChoiceList {
    named(
        &[
            "stop it and everything under it",
            "what it cost, and where its time went",
            "draw its graph",
            "the whole page, in a browser",
            "the run's whole chronicle",
            "start a new run",
            "every run of this repository",
            "the workflows you can run",
            "check adapters, git, forge, packs",
            "remove orphaned checkouts",
            "the shape of every document yunta reads",
        ],
        0,
    )
}

/// The palette's list in the cells and rows its box gives it at 80 by 24, whole, and at 60
/// by 14, where the blank row gives way and the rows under its window are counted.
#[test]
fn the_palettes_list_draws_the_targets_rows_at_each_size() {
    for (size, cells, rows) in [("unicode-80", 74, 13), ("unicode-60", 54, 7)] {
        let drawn: String = held(&acts(), cells, rows)
            .iter()
            .map(|row| format!("{row}\n"))
            .collect();
        assert_golden(&goldens().join(format!("palette.{size}.txt")), &drawn);
    }
}

/// The shapes `yunta schema` reads, each with what it is for.
fn documents() -> ChoiceList {
    let shape = |name: &str, said: &str| ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: None,
        key: None,
    };
    ChoiceList {
        choices: Some(Choices {
            rows: vec![
                shape(
                    "tasks",
                    "the plan: one task per independently verifiable unit of work",
                ),
                shape(
                    "spec",
                    "the tests a plan's tasks are held to, written before any task is built",
                ),
                shape(
                    "findings",
                    "what a review found, as data the engine counts and carries forward",
                ),
                shape("questions", "what a node needs a person to decide"),
                shape("answers", "what a person replied, as the engine records it"),
                shape("workflow", "the nodes a run executes, and how they connect"),
            ],
            at: 0,
        }),
        filter: None,
        legend: Legend::new(),
    }
}

/// A row's name stands in a column three cells past the widest name, and what it is for
/// beside it, cut at the row's end rather than wrapped: a row of a list is one row.
#[test]
fn what_a_row_is_for_stands_beside_its_name_cut_at_the_rows_end() {
    assert_eq!(
        held(&documents(), 78, usize::MAX)[2..],
        [
            "▸ tasks       the plan: one task per independently verifiable unit of work",
            "  spec        the tests a plan's tasks are held to, written before any task i…",
            "  findings    what a review found, as data the engine counts and carries forw…",
            "  questions   what a node needs a person to decide",
            "  answers     what a person replied, as the engine records it",
            "  workflow    the nodes a run executes, and how they connect",
        ]
    );
    assert_eq!(
        held(&documents(), 58, usize::MAX)[2..],
        [
            "▸ tasks       the plan: one task per independently verifi…",
            "  spec        the tests a plan's tasks are held to, writt…",
            "  findings    what a review found, as data the engine cou…",
            "  questions   what a node needs a person to decide",
            "  answers     what a person replied, as the engine record…",
            "  workflow    the nodes a run executes, and how they conn…",
        ]
    );
}

/// A list its filter empties says so at the column its names stand in, under the row what
/// was typed stands on.
#[test]
fn a_list_the_filter_empties_says_so_where_its_names_stand() {
    let empty = ChoiceList {
        choices: None,
        filter: Some("zz".to_string()),
        legend: Legend::new(),
    };
    assert_eq!(
        held(&empty, 40, usize::MAX),
        [" ▸ zz", "", "  nothing matches"]
    );
}

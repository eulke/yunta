//! A desk is a block every surface draws: whole in a document, as Markdown, as a
//! page, and on a terminal in every environment, with every group of its rail and
//! every line of its pane in sight; and with words typed at its foot, in a pane at the
//! sizes a screen gives it.

use std::path::{Path, PathBuf};

use super::{Desk, DeskFoot, FootRow, Pane, RailGroup, RailRow};
use crate::components::{ChoiceRow, Legend, Sight, TypedRow};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/desk")
}

/// A plan of two tasks at its desk, the first stood on, its proof being read, and
/// its three options open.
pub(crate) fn plan() -> Desk {
    Desk {
        title: vec![
            "plan".to_string(),
            "A32BZY".to_string(),
            "reviewed-plan".to_string(),
            "2 tasks".to_string(),
        ],
        edge: Some("needs you  approve-plan".to_string()),
        gist: Some("Make it".to_string()),
        asked: Some("a file the run is about".to_string()),
        rail: rail(),
        pane: proof(),
        foot: DeskFoot {
            question: "Plan registered. Approve?".to_string(),
            row: FootRow::Options {
                named: vec![
                    "approve".to_string(),
                    "adjust".to_string(),
                    "abort".to_string(),
                ],
                more: 0,
            },
            legend: Legend::new()
                .key("tab", "next place")
                .key("p", "proof")
                .key("esc", "leave it waiting"),
        },
    }
}

/// The places to stand on, what the plan says counted, and what weighs on it.
fn rail() -> Vec<RailGroup> {
    let stop = |lead: &str, name: &str, pointed: bool| RailRow::Stop {
        lead: lead.to_string(),
        name: name.to_string(),
        pointed,
    };
    vec![
        RailGroup {
            title: None,
            rows: vec![
                stop("", "the change", false),
                stop("1", "T001", true),
                stop("", "T002", false),
            ],
        },
        RailGroup {
            title: Some("what it says".to_string()),
            rows: vec![RailRow::Tally {
                what: "decided for you".to_string(),
                count: 2,
                key: Some('d'),
            }],
        },
        RailGroup {
            title: Some("what weighs on it".to_string()),
            rows: vec![RailRow::Weighs("the suite is red at baseline".to_string())],
        },
    ]
}

/// What proves the first task, being read.
fn proof() -> Pane {
    Pane {
        title: "proof · T001".to_string(),
        edge: Some("step 1 of 1".to_string()),
        place: None,
        doc: Doc::new().with(Block::Lines(vec![
            Line::new()
                .push(Tone::Muted, "tested by    ")
                .plain("the file exists"),
            Line::new()
                .plain("             ")
                .push(Tone::Strong, "test -f made.txt"),
        ])),
        sight: Sight::default(),
    }
}

#[test]
fn a_desk_is_written_whole_as_markdown_and_drawn_whole_on_a_page() {
    let doc = Doc::new().with(plan());
    assert_golden(&goldens().join("plan.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("plan.html"), &Html.body(&doc));
}

#[test]
fn a_desk_in_a_document_matches_its_goldens_in_every_environment() {
    let doc = Doc::new().with(plan());
    for environment in ENVIRONMENTS.iter().chain(&RICH) {
        let drawn = Terminal::on(Look::of(environment)).draw(&doc);
        assert_golden(&environment.golden(&goldens(), "plan"), &drawn);
    }
}

/// The plan's desk with words being typed for its `adjust`, `before` the cursor, as the
/// screen's target draws it: its keys in the screen's legend.
fn saying(before: &str) -> Desk {
    let mut saying = plan();
    saying.foot = DeskFoot {
        question: "Approve the plan: Add explicit per-user global pack installation alongside \
                   project-level installs?"
            .to_string(),
        row: FootRow::Saying {
            chosen: ChoiceRow {
                name: "adjust".to_string(),
                said: Some(
                    "Back to `plan`: sends what you say, and asks again once it completes"
                        .to_string(),
                ),
                cost: None,
                key: None,
            },
            asks: "what should change?".to_string(),
            typed: TypedRow {
                prompt: "> ".to_string(),
                before: before.to_string(),
                after: String::new(),
            },
            refused: None,
        },
        legend: Legend::new(),
    };
    saying
}

/// Words typed for an option stand at the foot in place of the options: the option with
/// what it does, then the box the words are typed in, what is asked of them faint in it
/// while nothing is typed, and the cursor on its middle row, after what was typed.
#[test]
fn a_desk_whose_foot_types_words_draws_the_box_in_place_of_its_options() {
    let placed = saying("").drawn(&in_a_pane(118), 32);
    let rows: Vec<String> = placed
        .lines
        .iter()
        .map(|row| row.text().trim_end().to_string())
        .collect();
    let level = "─".repeat(114);
    assert_eq!(
        rows[rows.len() - 6..],
        [
            String::new(),
            " ◆ Approve the plan: Add explicit per-user global pack installation alongside \
             project-level installs?"
                .to_string(),
            " adjust  Back to `plan`: sends what you say, and asks again once it completes"
                .to_string(),
            format!(" ╭{level}╮"),
            format!(" │ what should change?{} │", " ".repeat(93)),
            format!(" ╰{level}╯"),
        ]
    );
    assert_eq!(placed.cursor, Some((30, 3)));

    let typed = saying("keep the lock").drawn(&in_a_pane(118), 32);
    assert_eq!(
        typed.lines[30].text(),
        format!(" │ keep the lock{} │", " ".repeat(99))
    );
    assert_eq!(typed.cursor, Some((30, 3 + "keep the lock".len())));
}

/// At 60 by 14 the foot keeps the option and the box whole, its blank row given up and
/// its question and option cut at the line.
#[test]
fn a_short_narrow_desk_keeps_the_box_its_words_are_typed_in() {
    let placed = saying("").drawn(&in_a_pane(58), 10);
    let rows: Vec<String> = placed
        .lines
        .iter()
        .map(|row| row.text().trim_end().to_string())
        .collect();
    let level = "─".repeat(54);
    assert_eq!(
        rows[rows.len() - 5..],
        [
            " ◆ Approve the plan: Add explicit per-user global pack i…".to_string(),
            " adjust  Back to `plan`: sends what you say, and asks ag…".to_string(),
            format!(" ╭{level}╮"),
            format!(" │ what should change?{} │", " ".repeat(33)),
            format!(" ╰{level}╯"),
        ]
    );
    assert_eq!(placed.cursor, Some((8, 3)));
}

/// A pane as wide as `cells`, in Unicode, unpainted: a desk's place on a screen.
fn in_a_pane(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Moves,
    }
}

/// Words typed at a desk's foot, as Markdown and as a page: the option they are said
/// with, what is asked of them and the words as they stand.
#[test]
fn a_desk_typing_at_its_foot_is_written_as_markdown_and_drawn_on_a_page() {
    let doc = Doc::new().with(saying("keep the lock"));
    assert_golden(&goldens().join("saying.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("saying.html"), &Html.body(&doc));
}

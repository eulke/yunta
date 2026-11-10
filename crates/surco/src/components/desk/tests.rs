//! A desk is a block every surface draws: whole in a document, as Markdown, as a
//! page, and on a terminal in every environment, with every group of its rail and
//! every line of its pane in sight.

use std::path::{Path, PathBuf};

use super::{Desk, DeskFoot, FootRow, Pane, RailGroup, RailRow};
use crate::components::Legend;
use crate::diagram::Diagram;
use crate::doc::{Block, Doc};
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::line::{Line, Tone};
use crate::look::Look;
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
            "reviewed-plan (default)".to_string(),
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
                .key("j k", "task")
                .key("p", "the card")
                .key("esc", "leaves the run waiting"),
        },
    }
}

/// The places to stand on, the paths each task changes, and what weighs on the plan.
fn rail() -> Vec<RailGroup> {
    let stop = |lead: &str, name: &str, pointed: bool| RailRow::Stop {
        lead: lead.to_string(),
        name: name.to_string(),
        pointed,
    };
    let pair = |left: &str, right: &str| RailRow::Pair {
        left: left.to_string(),
        right: right.to_string(),
    };
    vec![
        RailGroup {
            title: None,
            rows: vec![stop("", "the change", false)],
        },
        RailGroup {
            title: Some("steps".to_string()),
            rows: vec![stop("1", "T001", true), stop("", "T002", false)],
        },
        RailGroup {
            title: Some("impact".to_string()),
            rows: vec![pair("made.txt", "T001"), pair("said.txt", "T002")],
        },
        RailGroup {
            title: Some("weighs".to_string()),
            rows: vec![
                RailRow::Tally {
                    what: "decided".to_string(),
                    count: 2,
                    key: Some('d'),
                },
                RailRow::Weighs("the suite is red at baseline".to_string()),
                RailRow::Order(Diagram::linked(&["T001", "T002"], &[(0, 1)])),
            ],
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
        top: 0,
        follows_pointer: false,
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

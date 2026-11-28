//! The blocks a run's closing is made of, for the catalog.

use super::*;

/// A change, file by file: a file that adds and removes, one that only adds, and
/// what the rest come to.
fn change_by_file() -> Diffstat {
    let file = |path: &str, added, removed| FileRow {
        path: path.to_string(),
        added,
        removed,
    };
    Diffstat {
        files: vec![
            file("crates/cli/src/pack.rs", 142, 38),
            file("docs/notes.md", 38, 0),
        ],
        more: Some(MoreFiles {
            files: 7,
            added: 232,
            removed: 49,
        }),
    }
}

/// What a run spent against the cap that stopped it, the cap and the spending with their
/// values in one column, then what it wants to go past it for, a fact with no value.
fn spent_against_its_cap() -> Labelled {
    let label = |words: &str, tone: Tone| Line::new().plain("  ").push(tone, words);
    Labelled {
        rows: vec![
            LabelledRow {
                label: label("tokens per run", Tone::Muted),
                value: Some("2.00M".to_string()),
            },
            LabelledRow {
                label: label("spent", Tone::Muted),
                value: Some("1.94M".to_string()),
            },
            LabelledRow {
                label: label("it wants more to review what it built", Tone::Plain),
                value: None,
            },
        ],
    }
}

/// The blocks a plan and a run's rows are drawn with, and how a document
/// closes: the decisions, the facts, and what to type next.
pub(super) fn closing() -> Vec<Block> {
    let column = |title, holds| Column { title, holds };
    let table = Table {
        columns: vec![
            column("step", Holds::Words),
            column("task", Holds::Id),
            column("", Holds::Rest),
        ],
        rows: vec![Row {
            mark: Some(Mark::Running),
            cells: vec![
                Cell::plain("1"),
                Cell::plain("cli-global"),
                Cell::plain("Global and project flags"),
            ],
        }],
    };
    let nodes = NodeTable {
        rows: planned_rows().into_iter().chain(fan_out_rows()).collect(),
    };
    vec![
        table.into(),
        nodes.into(),
        field().into(),
        change_by_file().into(),
        Block::Lines(vec![Line::new().plain("  a line a page composed itself")]),
        Block::Rows(vec![
            Line::new()
                .plain("  a row a pointer moves along, cut where it passes the edge it stands at"),
            Line::new().plain("  the next row"),
        ]),
        decision().into(),
        on_the_forge().into(),
        Note("the run holds its own state on disk.".to_string()).into(),
        Fields::new().push_if("tokens", "20.2k spent").into(),
        spent_against_its_cap().into(),
        Next {
            steps: vec![("yunta status 7E5PH4".to_string(), "where it stands")],
        }
        .into(),
    ]
}

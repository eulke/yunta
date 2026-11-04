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
        decision().into(),
        on_the_forge().into(),
        Note("the run holds its own state on disk.".to_string()).into(),
        Fields::new().push_if("tokens", "20.2k spent").into(),
        Next {
            steps: vec![("yunta status 7E5PH4".to_string(), "where it stands")],
        }
        .into(),
    ]
}

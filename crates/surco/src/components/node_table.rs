//! A run's nodes, one to a row: the mark and word for where each stands,
//! its id, a note, and what it took and spent.

use crate::components::{Cell, Column, Holds, Row, Table};
use crate::glyphs::Mark;
use crate::line::Tone;

/// One node's row.
pub struct NodeRow {
    pub mark: Mark,
    /// The word for where the node stands, in the one vocabulary.
    pub word: &'static str,
    pub id: String,
    /// What the node said or what stopped it, in one line.
    pub note: String,
    /// How long it worked; empty for a node that never started.
    pub took: String,
    /// What its attempts spent; empty for one that spent nothing.
    pub spent: String,
}

/// Every node of a run in a table whose id column is as wide as its
/// longest id, up to a third of the line: an id is what a reader types
/// next, and one cut to share a column is cut in its middle, where two
/// fan-out siblings never differ.
pub struct NodeTable {
    pub rows: Vec<NodeRow>,
}

impl NodeTable {
    /// The nodes as the table every surface draws: where each stands, its
    /// id, its note in what the line leaves, and — when any node has them
    /// — what each took and spent.
    pub fn table(&self) -> Table {
        let mut columns = vec![
            Column {
                title: "",
                holds: Holds::State,
            },
            Column {
                title: "node",
                holds: Holds::Id,
            },
            Column {
                title: "",
                holds: Holds::Rest,
            },
        ];
        // A column of figures nobody has takes no room from the notes.
        let took = self.rows.iter().any(|row| !row.took.is_empty());
        let spent = self.rows.iter().any(|row| !row.spent.is_empty());
        for (title, held) in [("time", took), ("tokens", spent)] {
            if held {
                columns.push(Column {
                    title,
                    holds: Holds::Figure,
                });
            }
        }
        let rows = self
            .rows
            .iter()
            .map(|row| {
                let mut cells = vec![
                    Cell::toned(Tone::of(row.mark), row.word),
                    Cell::plain(row.id.as_str()),
                    Cell::toned(Tone::Muted, row.note.as_str()),
                ];
                if took {
                    cells.push(Cell::plain(row.took.as_str()));
                }
                if spent {
                    cells.push(Cell::plain(row.spent.as_str()));
                }
                Row {
                    mark: Some(row.mark),
                    cells,
                }
            })
            .collect();
        Table { columns, rows }
    }
}

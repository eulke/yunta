//! A run's nodes, one to a row: the mark and word for where each stands,
//! its id, and a note.

use super::{Cell, Column, Drawn, Holds, Row, Table};
use crate::ink::{Line, Tone};
use crate::{Look, Mark};

/// One node's row.
pub struct NodeRow {
    pub mark: Mark,
    /// The word for where the node stands, in the one vocabulary.
    pub word: &'static str,
    pub id: String,
    /// What the node said or what stopped it, in one line.
    pub note: String,
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
    /// id, and its note in what the line leaves.
    pub fn table(&self) -> Table {
        Table {
            columns: vec![
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
            ],
            rows: self
                .rows
                .iter()
                .map(|row| Row {
                    mark: Some(row.mark),
                    cells: vec![
                        Cell::toned(Tone::of(row.mark), row.word),
                        Cell::plain(row.id.as_str()),
                        Cell::toned(Tone::Muted, row.note.as_str()),
                    ],
                })
                .collect(),
        }
    }
}

impl Drawn for NodeTable {
    fn lines(&self, look: &Look) -> Vec<Line> {
        self.table().lines(look)
    }
}

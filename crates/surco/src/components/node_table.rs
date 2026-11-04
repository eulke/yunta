//! A run's nodes, one to a row: the mark and word for where each stands,
//! its id with the branch it hangs from and where its failure returns, a
//! note, and what it took and spent.

use crate::components::{Cell, Column, Holds, Row, Table};
use crate::glyphs::{Glyphs, Mark};
use crate::line::Tone;

/// One node's row.
pub struct NodeRow {
    pub mark: Mark,
    /// The word for where the node stands, in the one vocabulary.
    pub word: &'static str,
    pub id: String,
    /// The branch the node hangs from when it is a child of a group; `None`
    /// for a node at the top.
    pub branch: Option<Branch>,
    /// The node its failure goes to, which then runs it again, as its
    /// `on_failure` declares; `None` for a node that declares none.
    pub returns: Option<String>,
    /// What the node said or what stopped it, in one line.
    pub note: String,
    /// How long it worked, or a person took; empty for a node that never
    /// started.
    pub took: String,
    /// Whether `took` is a person's time, waiting on an answer: it reads in
    /// the person's tone, and the note says it in words.
    pub took_by_person: bool,
    /// What its attempts spent; empty for one that spent nothing.
    pub spent: String,
}

/// The branch a child of a group hangs from in a list of nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    /// Every child but the group's last.
    Child,
    /// The group's last child.
    Last,
}

/// Every node of a run in a table whose id column is as wide as its
/// longest id, up to a third of the line: an id is what a reader types
/// next, and one cut to share a column is cut in its middle, where two
/// fan-out siblings never differ.
pub struct NodeTable {
    pub rows: Vec<NodeRow>,
}

impl NodeTable {
    /// The nodes as the table a surface drawing in `glyphs` draws: where
    /// each stands, its id after the branch it hangs from and before the
    /// node its failure returns to, its note in what the line leaves, and —
    /// when any node has them — what each took and spent, a person's time in
    /// the person's tone.
    pub fn table(&self, glyphs: Glyphs) -> Table {
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
            .map(|row| Row {
                mark: Some(row.mark),
                cells: row.cells(glyphs, took, spent),
            })
            .collect();
        Table { columns, rows }
    }
}

impl NodeRow {
    /// This row's cells in the columns a table lays out for `took` and
    /// `spent`: where the node stands, its id, its note, then each figure
    /// the table holds, a person's time in the person's tone.
    fn cells(&self, glyphs: Glyphs, took: bool, spent: bool) -> Vec<Cell> {
        let mut cells = vec![
            Cell::toned(Tone::of(self.mark), self.word),
            self.id_in(glyphs),
            Cell::toned(Tone::Muted, self.note.as_str()),
        ];
        if took {
            let tone = match self.took_by_person {
                true => Tone::NeedsYou,
                false => Tone::Plain,
            };
            cells.push(Cell::toned(tone, self.took.as_str()));
        }
        if spent {
            cells.push(Cell::plain(self.spent.as_str()));
        }
        cells
    }

    /// The row's id as `glyphs` draws it: after the branch it hangs from,
    /// and before the node its failure returns to.
    fn id_in(&self, glyphs: Glyphs) -> Cell {
        let branch = match self.branch {
            Some(Branch::Child) => format!("{} ", glyphs.branch(false)),
            Some(Branch::Last) => format!("{} ", glyphs.branch(true)),
            None => String::new(),
        };
        let returns = match &self.returns {
            Some(to) => format!(" {} {to}", glyphs.returns()),
            None => String::new(),
        };
        Cell::around(branch, &self.id, returns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row with every cell this table adds.
    fn row(branch: Option<Branch>, returns: Option<&str>, took_by_person: bool) -> NodeRow {
        NodeRow {
            mark: Mark::Absent,
            word: "never ran",
            id: "lint".to_string(),
            branch,
            returns: returns.map(str::to_string),
            note: String::new(),
            took: "6h27m".to_string(),
            took_by_person,
            spent: String::new(),
        }
    }

    /// A child's id hangs from its branch, and a node whose failure returns
    /// names where, each in the glyphs of the surface that draws it.
    #[test]
    fn an_id_hangs_from_its_branch_and_names_where_its_failure_returns() {
        let table = NodeTable {
            rows: vec![
                row(Some(Branch::Child), None, false),
                row(Some(Branch::Last), Some("fix-lint"), false),
            ],
        };
        let ids = |glyphs: Glyphs| -> Vec<String> {
            table
                .table(glyphs)
                .rows
                .iter()
                .map(|row| row.cells[1].text.clone())
                .collect()
        };
        assert_eq!(ids(Glyphs::Unicode), ["├─ lint", "└─ lint ↺ fix-lint"]);
        assert_eq!(ids(Glyphs::Ascii), ["+- lint", "`- lint <> fix-lint"]);
    }

    /// Of an id's cell, only the id is marked as the id: the branch and the
    /// return around it are words the cell holds, not what a reader types.
    #[test]
    fn only_the_id_of_a_row_is_marked_as_its_id() {
        let table = NodeTable {
            rows: vec![row(Some(Branch::Last), Some("fix-lint"), false)],
        };
        let drawn = table.table(Glyphs::Unicode);
        let cell = &drawn.rows[0].cells[1];
        let range = cell.id.clone().expect("an id cell marks its id");
        assert_eq!(&cell.text[range], "lint");
    }

    /// A person's time reads in the person's tone; a node's own work, plain.
    #[test]
    fn a_person_s_time_reads_in_the_person_s_tone() {
        let table = NodeTable {
            rows: vec![row(None, None, true), row(None, None, false)],
        };
        let tones: Vec<Tone> = table
            .table(Glyphs::Unicode)
            .rows
            .iter()
            .map(|row| row.cells[3].tone)
            .collect();
        assert_eq!(tones, [Tone::NeedsYou, Tone::Plain]);
    }
}

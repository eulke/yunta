//! A change, file by file, as a forge renders it.

use super::{table, GLYPHS};
use crate::components::{Cell, Column, Diffstat, Holds, Row, Table, COLUMNS};
use crate::figures::signed_lines;

/// A change, file by file, as a table a forge renders, and what the rest come to.
pub(super) fn diffstat(stat: &Diffstat) -> String {
    if stat.files.is_empty() {
        return String::new();
    }
    let [path, added, removed] = COLUMNS;
    let column = |title, holds| Column { title, holds };
    let files = table(&Table {
        columns: vec![
            column(path, Holds::Id),
            column(added, Holds::Figure),
            column(removed, Holds::Figure),
        ],
        rows: stat
            .files
            .iter()
            .map(|file| {
                let [added, removed] = signed_lines(file.added, file.removed, GLYPHS);
                Row {
                    mark: None,
                    cells: vec![
                        Cell::plain(&file.path),
                        Cell::plain(added),
                        Cell::plain(removed),
                    ],
                }
            })
            .collect(),
    });
    match &stat.more {
        Some(more) => format!("{files}\n\n{}", more.said(GLYPHS)),
        None => files,
    }
}

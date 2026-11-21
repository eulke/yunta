//! Facts on labelled rows, each value after its label: the labels in one column two cells past
//! the widest of the rows that stand, a fact that holds no value standing in that column and
//! counted among its widest. At a width where none of a row's value can stand in its row after
//! its label, as a figure the cut takes whole since it never splits a number, the row says
//! nothing and gives way, and the column fits the rows that stand: a label with no value reads
//! as a fact not said. A value of words of which some stands is cut with the rest of its row.

use crate::line::Line;

/// Facts on labelled rows, the labels in one column two cells past the widest of the rows that
/// stand. Not [`super::Fields`], whose labels stand in a column of a fixed width and whose
/// long values wrap under it.
#[derive(Debug, Clone, Default)]
pub struct Labelled {
    pub rows: Vec<LabelledRow>,
}

/// One labelled row: its label, as its marks and tones say it, and its value, where it holds
/// one; a row with none is a fact said in the labels' column.
#[derive(Debug, Clone)]
pub struct LabelledRow {
    pub label: Line,
    pub value: Option<String>,
}

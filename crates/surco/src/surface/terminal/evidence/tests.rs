//! The end of what a command printed cut from its top, as a pane short of rows keeps it: the
//! lines above those it keeps counted at their place in the gutter, where every list that hides
//! rows counts them, and no other word of them.

use crate::components::{Evidence, Whole};
use crate::line::Line;
use crate::look::Look;

/// `evidence` drawn plain, a line each.
fn plain(evidence: &Evidence) -> Vec<String> {
    evidence
        .lines(&Look::plain())
        .iter()
        .map(Line::text)
        .collect()
}

/// Lines 1 to `lines` of what a command printed, its whole output a command away.
fn printed(lines: usize, kept: Option<usize>) -> Evidence {
    Evidence {
        tail: (1..=lines).map(|n| format!("line {n}")).collect(),
        whole: Some(Whole::Command(
            "yunta status 7E5PH4 --node lint".to_string(),
        )),
        kept,
    }
}

#[test]
fn cut_from_its_top_it_counts_every_line_above_those_it_keeps_in_the_gutter() {
    assert_eq!(
        plain(&printed(6, Some(1))),
        [
            "  | ... 5 above",
            "  | line 6",
            "  whole output: yunta status 7E5PH4 --node lint"
        ]
    );
    assert_eq!(
        plain(&printed(20, Some(2)))[..3],
        ["  | ... 18 above", "  | line 19", "  | line 20"]
    );
}

#[test]
fn keeping_every_line_it_quotes_it_cuts_none() {
    assert_eq!(plain(&printed(3, Some(4))), plain(&printed(3, None)));
}

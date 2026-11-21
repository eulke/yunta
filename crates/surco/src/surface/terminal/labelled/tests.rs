//! Labelled rows: the values two cells past the widest label of the rows that stand; a row
//! whose value cannot stand in its row gives way, and the column fits those left.

use crate::components::{Labelled, LabelledRow};
use crate::glyphs::Glyphs;
use crate::line::Line;

/// A row of `label`, with `value` where it holds one.
fn row(label: &str, value: Option<&str>) -> LabelledRow {
    LabelledRow {
        label: Line::new().plain(label),
        value: value.map(str::to_string),
    }
}

/// A run's token cap and what it spent, as a card on it rests on them.
fn the_cap() -> Labelled {
    Labelled {
        rows: vec![
            row("  limits.max_tokens_per_run", Some("100")),
            row(
                "  total input+output tokens derived from the event log",
                Some("200"),
            ),
        ],
    }
}

/// `labelled` laid out in `cells`, each row's words.
fn laid(labelled: &Labelled, cells: usize) -> Vec<String> {
    labelled
        .laid(cells, Glyphs::Unicode)
        .iter()
        .map(Line::text)
        .collect()
}

#[test]
fn the_values_stand_two_cells_past_the_widest_label() {
    assert_eq!(
        laid(&the_cap(), 84),
        [
            "  limits.max_tokens_per_run                             100",
            "  total input+output tokens derived from the event log  200",
        ]
    );
}

#[test]
fn a_row_whose_value_cannot_stand_gives_way_and_the_column_fits_those_left() {
    assert_eq!(laid(&the_cap(), 58), ["  limits.max_tokens_per_run  100"]);
}

#[test]
fn a_fact_with_no_value_stands_in_the_column_and_counts_among_its_widest() {
    let labelled = Labelled {
        rows: vec![
            row("  limits.max_loop_iterations", Some("2")),
            row("  the tasks document still has ready tasks", None),
        ],
    };
    assert_eq!(
        laid(&labelled, 84),
        [
            "  limits.max_loop_iterations                2",
            "  the tasks document still has ready tasks",
        ]
    );
}

#[test]
fn a_value_of_words_some_of_which_stands_keeps_its_row_for_the_surface_to_cut() {
    let labelled = Labelled {
        rows: vec![
            row("● build", Some("attempt 1, after 0s")),
            row(
                "why",
                Some("no process is driving it: the engine that ran it is gone"),
            ),
        ],
    };
    assert_eq!(
        laid(&labelled, 58),
        [
            "● build  attempt 1, after 0s",
            "why      no process is driving it: the engine that ran it is gone",
        ]
    );
}

#[test]
fn laid_out_with_no_bound_every_row_stands_whole() {
    assert_eq!(laid(&the_cap(), usize::MAX), laid(&the_cap(), 84));
}

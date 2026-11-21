//! Where each part of a screen stands, each rule with the numbers of a terminal it is
//! worked out on by hand.

use ratatui::layout::Rect;

use surco::components::SideAt;

use super::{areas, Areas, Beside, Boxed, Holds};

/// The cells of Surco's edge between two regions: ` │ `.
const EDGE: u16 = 3;

fn terminal(columns: u16, rows: u16) -> Rect {
    Rect::new(0, 0, columns, rows)
}

/// A run's screen: what stands across it in `about` rows, its nodes beside its page at the
/// left.
fn a_run(about: u16) -> Holds {
    Holds {
        about,
        side: Some(Beside {
            at: SideAt::Left,
            edge: EDGE,
            titled: false,
        }),
        main_titled: false,
        meter: false,
        overlay: None,
    }
}

/// A screen of runs: the list a person moves along, what the one pointed at says read
/// beside it at the right.
fn a_field() -> Holds {
    Holds {
        about: 0,
        side: Some(Beside {
            at: SideAt::Right,
            edge: EDGE,
            titled: false,
        }),
        main_titled: false,
        meter: false,
        overlay: None,
    }
}

fn laid(columns: u16, rows: u16, holds: Holds) -> Areas {
    areas(terminal(columns, rows), holds).expect("a terminal the application lays out on")
}

/// At 120 columns a side at the left takes 31 cells inside the margin, the edge 3 with its
/// rule in the 34th column, and the main region the 84 cells to the right margin.
#[test]
fn a_side_at_the_left_stands_inside_the_left_margin() {
    let wide = laid(120, 36, a_run(4));
    assert_eq!(wide.side, Some(Rect::new(1, 7, 31, 27)));
    assert_eq!(wide.edge, Some(Rect::new(32, 7, 3, 27)));
    assert_eq!(wide.main, Rect::new(35, 7, 84, 27));
}

/// At 120 columns a side at the right takes 39 cells up to the right margin, the edge 3
/// before it with its rule in the 79th column, and the main region the 76 cells from the
/// left margin.
#[test]
fn a_side_at_the_right_stands_at_the_right_edge() {
    let wide = laid(120, 36, a_field());
    assert_eq!(wide.main, Rect::new(1, 2, 76, 32));
    assert_eq!(wide.edge, Some(Rect::new(77, 2, 3, 32)));
    assert_eq!(wide.side, Some(Rect::new(80, 2, 39, 32)));
}

/// A side region stands from a terminal of 102 columns, at either edge; at 101 it gives
/// way and the main region takes every cell inside the margins.
#[test]
fn a_side_stands_at_102_columns_and_gives_way_at_101() {
    let left = laid(102, 24, a_run(4));
    assert_eq!(left.side, Some(Rect::new(1, 7, 31, 15)));
    assert_eq!(left.main, Rect::new(35, 7, 66, 15));
    let right = laid(102, 24, a_field());
    assert_eq!(right.main, Rect::new(1, 2, 58, 20));
    assert_eq!(right.side, Some(Rect::new(62, 2, 39, 20)));
    for holds in [a_run(4), a_field()] {
        let narrow = laid(101, 24, holds);
        assert_eq!(narrow.side, None);
        assert_eq!(narrow.edge, None);
        assert_eq!(narrow.main.x, 1);
        assert_eq!(narrow.main.width, 99);
    }
    let least = laid(60, 14, a_run(4));
    assert_eq!(least.side, None);
    assert_eq!(least.main, Rect::new(1, 6, 58, 6));
}

/// Wider than 120 columns a side keeps its width, at either edge, and the main region
/// takes the rest.
#[test]
fn wider_a_side_keeps_its_width() {
    let left = laid(140, 36, a_run(4));
    assert_eq!(left.side, Some(Rect::new(1, 7, 31, 27)));
    assert_eq!(left.main, Rect::new(35, 7, 104, 27));
    let right = laid(140, 36, a_field());
    assert_eq!(right.main, Rect::new(1, 2, 96, 32));
    assert_eq!(right.side, Some(Rect::new(100, 2, 39, 32)));
}

/// The rows from the top: the trail, the rule across every cell, what the screen is
/// about, a blank row, the regions, the row over the legend, the legend on the last row.
#[test]
fn the_rows_stand_from_the_trail_to_the_legend() {
    let areas = laid(80, 24, a_run(4));
    assert_eq!(areas.trail, Rect::new(1, 0, 78, 1));
    assert_eq!(areas.rule, Rect::new(0, 1, 80, 1));
    assert_eq!(areas.about, Some(Rect::new(1, 2, 78, 4)));
    assert_eq!(areas.attention, Rect::new(1, 22, 78, 1));
    assert_eq!(areas.legend, Rect::new(1, 23, 78, 1));
    let bare = laid(80, 24, a_run(0));
    assert_eq!(bare.about, None);
    assert_eq!(bare.main.y, 2, "with nothing about, no blank row under it");
    assert_eq!(
        bare.main.height, 20,
        "the row over the legend kept, nothing in it"
    );
}

/// A title takes the first row of its region's column, and the edge stands beside it too,
/// at either edge.
#[test]
fn a_title_takes_the_first_row_of_its_column() {
    let titled = |at| Holds {
        side: Some(Beside {
            at,
            edge: EDGE,
            titled: true,
        }),
        main_titled: true,
        ..a_run(2)
    };
    let left = laid(120, 36, titled(SideAt::Left));
    assert_eq!(left.side_title, Some(Rect::new(1, 5, 31, 1)));
    assert_eq!(left.main_title, Some(Rect::new(35, 5, 84, 1)));
    assert_eq!(left.edge, Some(Rect::new(32, 5, 3, 29)));
    assert_eq!(left.main, Rect::new(35, 6, 84, 28));
    let right = laid(120, 36, titled(SideAt::Right));
    assert_eq!(right.main_title, Some(Rect::new(1, 5, 76, 1)));
    assert_eq!(right.side_title, Some(Rect::new(80, 5, 39, 1)));
    assert_eq!(right.edge, Some(Rect::new(77, 5, 3, 29)));
    assert_eq!(right.side, Some(Rect::new(80, 6, 39, 28)));
}

/// A screen with nothing across it stands short of rows on the least terminal's rows, where
/// a page in its main region lets its blank rows go, and on a taller one does not; one with
/// something across it stands short only once the blank row under it gave way.
#[test]
fn a_screen_with_nothing_across_it_stands_short_of_rows_on_the_least_terminal() {
    assert!(laid(60, 14, a_field()).short_of_rows);
    assert!(!laid(80, 24, a_field()).short_of_rows);
    assert!(!laid(120, 36, a_field()).short_of_rows);
    assert!(laid(60, 14, a_run(4)).short_of_rows);
    assert!(!laid(80, 24, a_run(4)).short_of_rows);
}

/// Short of rows, what the screen is about gives way before the regions keep their fewest:
/// first the blank row under it, then its rows; the regions keep six, as a run's pane does
/// at the least terminal.
#[test]
fn short_of_rows_what_the_screen_is_about_gives_way_before_the_regions() {
    let blank_gone = laid(60, 14, a_run(4));
    assert_eq!(blank_gone.about, Some(Rect::new(1, 2, 58, 4)));
    assert_eq!(
        blank_gone.main,
        Rect::new(1, 6, 58, 6),
        "no blank row over it"
    );
    let metered = laid(
        60,
        14,
        Holds {
            meter: true,
            ..a_run(4)
        },
    );
    assert_eq!(
        metered.about.map(|about| about.height),
        Some(3),
        "a row of it let go"
    );
    assert_eq!(metered.main, Rect::new(1, 5, 58, 6));
    let short = laid(60, 14, a_run(12));
    assert_eq!(short.about.map(|about| about.height), Some(4));
    assert_eq!(short.main.height, 6);
    let tall = laid(
        80,
        24,
        Holds {
            meter: true,
            ..a_run(4)
        },
    );
    assert_eq!(tall.about.map(|about| about.height), Some(4), "whole");
    assert_eq!(
        tall.main,
        Rect::new(1, 7, 78, 14),
        "the regions take the rest"
    );
}

/// A box stands centred, as wide as it needs and 2 inside each edge, between the rule and
/// the meter's row, its top a third of the way down the rows they leave: what the run spent,
/// the row over the legend and the legend stay in sight under it.
#[test]
fn a_box_stands_centred_between_the_rule_and_the_meters_row() {
    let short = laid(
        80,
        24,
        Holds {
            overlay: Some(Boxed {
                columns: 60,
                rows: 6,
            }),
            ..a_run(2)
        },
    );
    assert_eq!(short.boxed, Some(Rect::new(10, 6, 60, 6)));
    let tall = laid(
        60,
        14,
        Holds {
            overlay: Some(Boxed {
                columns: 84,
                rows: 32,
            }),
            ..a_run(2)
        },
    );
    assert_eq!(tall.boxed, Some(Rect::new(2, 2, 56, 9)));
}

/// Under 60 columns or 14 rows, nothing is laid out: the screen is the room it needs.
#[test]
fn under_the_least_terminal_nothing_is_laid_out() {
    assert_eq!(areas(terminal(59, 14), a_run(2)), None);
    assert_eq!(areas(terminal(60, 13), a_run(2)), None);
    assert!(areas(terminal(60, 14), a_run(2)).is_some());
}

/// A title of the side region takes no row where the side region gives way.
#[test]
fn a_side_title_goes_with_its_region() {
    let titled = Holds {
        side: Some(Beside {
            at: SideAt::Left,
            edge: EDGE,
            titled: true,
        }),
        ..a_run(2)
    };
    let narrow = laid(60, 14, titled);
    assert_eq!(narrow.side_title, None);
    assert_eq!(narrow.main_title, None);
    assert_eq!(narrow.main.y, 5, "no row for a title");
}

/// A run's screen on the design's terminal: what stands across it in four rows (its
/// headline, its ribbon's names over its strokes, its gist), its nodes beside its pane at
/// the left, the pane under its title, and what the run spent.
fn a_run_titled() -> Holds {
    Holds {
        main_titled: true,
        meter: true,
        ..a_run(4)
    }
}

/// The meter stands on the row over the attention row's, inside the margins; the row over
/// the legend is kept whether or not anything stands in it.
#[test]
fn a_screen_holding_a_meter_lays_it_out_over_the_attention_row() {
    let wide = laid(120, 36, a_run_titled());
    assert_eq!(wide.meter, Some(Rect::new(1, 33, 118, 1)));
    assert_eq!(wide.attention, Rect::new(1, 34, 118, 1));
    assert_eq!(wide.legend, Rect::new(1, 35, 118, 1));
    let least = laid(60, 14, a_run_titled());
    assert_eq!(least.meter, Some(Rect::new(1, 11, 58, 1)));
    let none = laid(
        120,
        36,
        Holds {
            meter: false,
            ..a_run_titled()
        },
    );
    assert_eq!(none.meter, None);
    assert_eq!(none.attention, Rect::new(1, 34, 118, 1));
}

/// A main title stands on the main column's first row, the 8th, while the side region's
/// first row, a node, stands beside it, and the edge runs down both.
#[test]
fn a_main_title_stands_on_the_main_columns_first_row_beside_the_sides_first_node() {
    let wide = laid(120, 36, a_run_titled());
    assert_eq!(wide.main_title, Some(Rect::new(35, 7, 84, 1)));
    assert_eq!(wide.main, Rect::new(35, 8, 84, 25));
    assert_eq!(wide.side, Some(Rect::new(1, 7, 31, 26)));
    assert_eq!(wide.edge, Some(Rect::new(32, 7, 3, 26)));
}

/// A box stands as wide as it needs, up to 84 cells, and never wider than the terminal less
/// the cells inside its edges.
#[test]
fn a_box_is_never_wider_than_84_cells() {
    let over = |columns, rows| Holds {
        overlay: Some(Boxed { columns, rows }),
        ..a_run_titled()
    };
    let width =
        |terminal: (u16, u16), holds| laid(terminal.0, terminal.1, holds).boxed.map(|at| at.width);
    assert_eq!(width((120, 36), over(118, 15)), Some(84));
    assert_eq!(width((140, 36), over(138, 15)), Some(84));
    assert_eq!(width((120, 36), over(70, 19)), Some(70));
    assert_eq!(width((80, 24), over(118, 15)), Some(76));
    assert_eq!(width((60, 14), over(70, 19)), Some(56));
}

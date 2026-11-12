//! Where each part of a screen stands, each rule with the numbers of a terminal it is
//! worked out on by hand.

use ratatui::layout::Rect;

use super::{areas, Areas, Beside, Holds};

/// The cells of Surco's edge between two regions: ` │ `.
const EDGE: u16 = 3;

fn terminal(columns: u16, rows: u16) -> Rect {
    Rect::new(0, 0, columns, rows)
}

/// A run's screen: its ribbon and the rows it is watched by across it, its nodes beside
/// its page, something that needs the person.
fn a_run(about: u16) -> Holds {
    Holds {
        about,
        side: Some(Beside {
            edge: EDGE,
            titled: false,
        }),
        main_titled: false,
        attention: true,
        overlay: None,
    }
}

fn laid(columns: u16, rows: u16, holds: Holds) -> Areas {
    areas(terminal(columns, rows), holds).expect("a terminal the application lays out on")
}

/// At 80 columns the side region takes 22 cells and the edge 3 beside the main one, a
/// margin at each side; at 60 the main one would get fewer than 40, so the side gives way.
#[test]
fn the_side_region_stands_at_80_columns_and_gives_way_at_60() {
    let wide = laid(80, 24, a_run(4));
    assert_eq!(wide.side, Some(Rect::new(1, 7, 22, 15)));
    assert_eq!(wide.edge, Some(Rect::new(23, 7, 3, 15)));
    assert_eq!(wide.main, Rect::new(26, 7, 53, 15));
    let narrow = laid(60, 14, a_run(4));
    assert_eq!(narrow.side, None);
    assert_eq!(narrow.edge, None);
    assert_eq!(narrow.main, Rect::new(1, 7, 58, 5));
}

/// The rows from the top: the trail, the rule across every cell, what the screen is
/// about, a blank row, the regions, the attention, the legend on the last row.
#[test]
fn the_rows_stand_from_the_trail_to_the_legend() {
    let areas = laid(80, 24, a_run(4));
    assert_eq!(areas.trail, Rect::new(1, 0, 78, 1));
    assert_eq!(areas.rule, Rect::new(0, 1, 80, 1));
    assert_eq!(areas.about, Some(Rect::new(1, 2, 78, 4)));
    assert_eq!(areas.attention, Some(Rect::new(1, 22, 78, 1)));
    assert_eq!(areas.legend, Rect::new(1, 23, 78, 1));
    let bare = laid(
        80,
        24,
        Holds {
            about: 0,
            attention: false,
            ..a_run(0)
        },
    );
    assert_eq!(bare.about, None);
    assert_eq!(bare.attention, None);
    assert_eq!(bare.main.y, 2, "with nothing about, no blank row under it");
    assert_eq!(bare.main.height, 21);
}

/// A title takes a row over the regions, and the edge stands beside it too.
#[test]
fn a_title_takes_a_row_over_the_regions() {
    let titled = Holds {
        side: Some(Beside {
            edge: EDGE,
            titled: true,
        }),
        main_titled: true,
        ..a_run(2)
    };
    let areas = laid(80, 24, titled);
    assert_eq!(areas.side_title, Some(Rect::new(1, 5, 22, 1)));
    assert_eq!(areas.main_title, Some(Rect::new(26, 5, 53, 1)));
    assert_eq!(areas.edge, Some(Rect::new(23, 5, 3, 17)));
    assert_eq!(areas.main, Rect::new(26, 6, 53, 16));
}

/// Short of rows, the regions give way to their fewest, then what the screen is about
/// gives way from its last row.
#[test]
fn short_of_rows_the_regions_give_way_then_what_the_screen_is_about() {
    let fits = laid(60, 14, a_run(6));
    assert_eq!(fits.main.height, 3, "the regions down to their fewest rows");
    assert_eq!(fits.about.map(|about| about.height), Some(6));
    let short = laid(60, 14, a_run(12));
    assert_eq!(short.main.height, 3);
    assert_eq!(short.about.map(|about| about.height), Some(6), "about cut");
}

/// A box stands at most 60 cells wide and 2 inside each edge, centred, between the rule
/// and the legend row, its top a third of the way down the rows they leave.
#[test]
fn a_box_stands_centred_between_the_rule_and_the_legend() {
    let short = laid(
        80,
        24,
        Holds {
            overlay: Some(6),
            ..a_run(2)
        },
    );
    assert_eq!(short.boxed, Some(Rect::new(10, 6, 60, 6)));
    let tall = laid(
        60,
        14,
        Holds {
            overlay: Some(32),
            ..a_run(2)
        },
    );
    assert_eq!(tall.boxed, Some(Rect::new(2, 2, 56, 11)));
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

//! Where each part of a screen stands on a terminal, by the reference's rules, laid out
//! with Ratatui's `Layout`. Pure: a terminal's size and what a screen holds that takes
//! room go in, the area of each part comes out. What each part draws is Surco's, and how
//! many rows what a screen is about takes is measured from Surco's lines by the caller.
//!
//! Rows, from the top: the trail with the tally, the rule, what the screen is about, a
//! blank row under it, the regions' titles where a region has one, the regions, the
//! attention, the legend. Short of rows, the regions give way to their fewest, then what
//! the screen is about, from its last row. Columns: a margin at each side, but for the
//! rule, which takes every cell; the side region beside the main one, giving way where the
//! main one would be too narrow. What is open over the screen stands in a box between the
//! rule and the legend row. Under the least terminal, nothing is laid out: the screen is
//! the room it needs.

use ratatui::layout::{Constraint, Layout, Rect};

/// The cells a margin takes at each side of the terminal: a cell between what is read and
/// the terminal's edge.
const MARGIN: u16 = 1;

/// The cells the side region takes: a node's mark, its name and how long it worked, as
/// the reference lists a run's nodes.
const SIDE: u16 = 22;

/// The fewest cells the main region keeps before the side region gives way: under them a
/// page's lines wrap every few words.
const LEAST_MAIN: u16 = 40;

/// The widest a box stands: a line read whole, as a document's measure is.
const BOX_MOST: u16 = 60;

/// The cells a box stands inside each edge of the terminal, where what is under it shows.
const BOX_INSIDE: u16 = 2;

/// The rows a box's edges take around what it holds.
const BOX_EDGES: u16 = 2;

/// The rows that stand on every screen whatever it holds: the trail, the rule and the
/// legend.
const ALWAYS: u16 = 3;

/// The fewest rows a region keeps before what the screen is about gives way: the line a
/// pointer stands on, with one above it and one below.
const LEAST_REGION: u16 = 3;

/// The fewest columns the application lays a screen out on: the narrower terminal of the
/// reference.
pub(crate) const LEAST_COLUMNS: u16 = 60;

/// The fewest rows the application lays a screen out on: the shorter terminal of the
/// reference.
pub(crate) const LEAST_ROWS: u16 = 14;

/// What a screen holds that takes room on a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Holds {
    /// The rows what the screen is about takes at the inner width; none for nothing about.
    pub(crate) about: u16,
    /// The region beside the main one, where there is one.
    pub(crate) side: Option<Beside>,
    /// Whether the main region has a title, which takes a row over the regions.
    pub(crate) main_titled: bool,
    /// Whether something needs the person, which takes the row over the legend.
    pub(crate) attention: bool,
    /// The rows what is open over the screen takes inside its box, where something is.
    pub(crate) overlay: Option<u16>,
}

/// A region beside the main one: the cells of the edge between them, and whether it has a
/// title, which takes a row over the regions where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Beside {
    pub(crate) edge: u16,
    pub(crate) titled: bool,
}

/// Where each part of a screen stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Areas {
    pub(crate) trail: Rect,
    pub(crate) rule: Rect,
    pub(crate) about: Option<Rect>,
    pub(crate) side_title: Option<Rect>,
    pub(crate) side: Option<Rect>,
    pub(crate) edge: Option<Rect>,
    pub(crate) main_title: Option<Rect>,
    pub(crate) main: Rect,
    pub(crate) attention: Option<Rect>,
    pub(crate) legend: Rect,
    pub(crate) boxed: Option<Rect>,
}

/// Where each part of a screen holding `holds` stands on a terminal of `size`; none under
/// the least terminal, where the screen is the room it needs.
pub(crate) fn areas(size: Rect, holds: Holds) -> Option<Areas> {
    if size.width < LEAST_COLUMNS || size.height < LEAST_ROWS {
        return None;
    }
    let inner = size.inner(ratatui::layout::Margin::new(MARGIN, 0));
    let side = holds
        .side
        .filter(|side| inner.width >= SIDE + side.edge + LEAST_MAIN);
    let titled = holds.main_titled || side.is_some_and(|side| side.titled);
    let rows = Rows::of(size.height, holds, titled);
    let [trail, rule, about, _, titles, regions, attention, legend] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(rows.about),
        Constraint::Length(u16::from(rows.about > 0)),
        Constraint::Length(u16::from(titled)),
        Constraint::Length(rows.regions),
        Constraint::Length(u16::from(holds.attention)),
        Constraint::Length(1),
    ])
    .areas(size);
    let inside = |area: Rect| area.inner(ratatui::layout::Margin::new(MARGIN, 0));
    let edge_cells = side.map(|side| side.edge);
    let (side_area, main) = beside(inside(regions), edge_cells);
    let (side_title, main_title) = beside(inside(titles), edge_cells);
    let edge = side_area.zip(edge_cells).map(|(side, cells)| {
        let top = if titled { titles.y } else { regions.y };
        Rect::new(side.right(), top, cells, regions.bottom() - top)
    });
    Some(Areas {
        trail: inside(trail),
        rule,
        about: (rows.about > 0).then(|| inside(about)),
        side_title: side_title.filter(|_| side.is_some_and(|side| side.titled)),
        side: side_area,
        edge,
        main_title: holds.main_titled.then_some(main_title),
        main,
        attention: holds.attention.then(|| inside(attention)),
        legend: inside(legend),
        boxed: holds.overlay.map(|inside_rows| boxed(size, inside_rows)),
    })
}

/// The rows what the screen is about and the regions take on a terminal `height` rows
/// high: the regions give way first, to their fewest, then what the screen is about,
/// from its last row, the blank row under it with its last.
struct Rows {
    about: u16,
    regions: u16,
}

impl Rows {
    fn of(height: u16, holds: Holds, titled: bool) -> Self {
        let taken = ALWAYS + u16::from(holds.attention) + u16::from(titled);
        let left = height.saturating_sub(taken);
        let about_with_blank = if holds.about > 0 { holds.about + 1 } else { 0 };
        let about = match left.saturating_sub(about_with_blank) >= LEAST_REGION {
            true => holds.about,
            // What the screen is about keeps what the regions' fewest rows leave, less the
            // blank row under it; none at all where that is no row.
            false => left.saturating_sub(LEAST_REGION).saturating_sub(1),
        };
        let about_with_blank = if about > 0 { about + 1 } else { 0 };
        Rows {
            about,
            regions: left.saturating_sub(about_with_blank),
        }
    }
}

/// `area` shared between the side region, where one stands with an edge of `edge` cells
/// beside the main one, and the main region.
fn beside(area: Rect, edge: Option<u16>) -> (Option<Rect>, Rect) {
    match edge {
        Some(edge) => {
            let [side, _, main] = Layout::horizontal([
                Constraint::Length(SIDE),
                Constraint::Length(edge),
                Constraint::Fill(1),
            ])
            .areas(area);
            (Some(side), main)
        }
        _ => (None, area),
    }
}

/// The box what is open over a terminal of `size` stands in, holding `inside` rows:
/// centred, at most [`BOX_MOST`] cells and [`BOX_INSIDE`] inside each edge; between the
/// rule and the legend row, its top a third of the way down the rows it leaves and never
/// above the row under the rule.
fn boxed(size: Rect, inside: u16) -> Rect {
    let columns = BOX_MOST.min(size.width.saturating_sub(2 * BOX_INSIDE));
    let rows = (inside + BOX_EDGES).min(size.height.saturating_sub(ALWAYS));
    let top = ((size.height - rows) / 3).max(2);
    Rect::new(
        size.x + (size.width - columns) / 2,
        size.y + top,
        columns,
        rows,
    )
}

/// The cells a box stands in on a terminal `columns` wide: what is open over it is laid
/// out at that width, less its edges, before the box's rows are known.
pub(crate) fn box_columns(columns: u16) -> u16 {
    BOX_MOST.min(columns.saturating_sub(2 * BOX_INSIDE))
}

#[cfg(test)]
mod tests;

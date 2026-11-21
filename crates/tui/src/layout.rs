//! Where each part of a screen stands on a terminal, laid out with Ratatui's `Layout`.
//! Pure: a terminal's size and what a screen holds that takes room go in, the area of each
//! part comes out. What each part draws is Surco's, and how many rows what a screen is
//! about takes, and how wide its box needs to be, are measured from Surco's lines by the
//! caller.
//!
//! Rows, from the top: the trail with the tally, the rule, what the screen is about, a
//! blank row under it, the regions, the meter where the screen says what its run spent, the
//! row over the legend, and the legend. The row over the legend stands on every screen,
//! blank where neither a note nor what needs the person stands in it. A region's title is
//! the first row of its own column: the main region's stands beside the side region's first
//! row, never on a row of its own across both. Short of rows, what the screen is about gives
//! way before the regions keep their fewest: first the blank row under it, then its rows,
//! which its drawing lets go in its own order; taller, the regions take every row it leaves.
//! A screen whose blank row under what stands across gave way stands short of rows, which the
//! meter's drawing reads: there it says its figures alone; so does a screen with nothing
//! across it on the least terminal's rows, where a page in its main region lets its blank
//! rows go.
//!
//! Columns: a margin at each side, but for the rule, which takes every cell; the side region
//! at the edge its screen names, the left (a run's nodes, 31 cells) or the right (what is
//! read beside a list, 39 cells), with the edge between it and the main region. It stands
//! from a terminal of 102 columns and keeps its width wider, the main region taking the
//! rest; below 102 it gives way whole and the main region takes every cell inside the
//! margins. What is open over the screen stands in a box between the rule and the meter's
//! row, as wide as it needs up to [`BOX_MOST`], so the rows under the regions stay in sight
//! under it. Under the least terminal, nothing is laid out: the screen is the room it needs.

use ratatui::layout::{Constraint, Layout, Rect};
use surco::components::SideAt;

/// The cells a margin takes at each side of the terminal: a cell between what is read and
/// the terminal's edge.
const MARGIN: u16 = 1;

/// The fewest columns a side region stands on: the design's screen, 100 cells inside its
/// margins, the narrowest it draws a region beside the main one on.
const SIDE_FROM: u16 = 102;

/// The cells a side region at the left takes: a run's nodes, one to a row, each with its
/// pointer, its mark, a name of 19 cells and how long it worked; a longer name is cut at
/// the region's edge.
const SIDE_AT_LEFT: u16 = 31;

/// The cells a side region at the right takes: what is read beside a list, a label's
/// column and its value, which wraps under its own first column.
const SIDE_AT_RIGHT: u16 = 39;

/// The widest a box stands: the palette's three columns, what each act does, its key and
/// the command that does the same, as a terminal of 120 columns draws them.
pub(crate) const BOX_MOST: u16 = 84;

/// The cells a box stands inside each edge of the terminal, where what is under it shows.
const BOX_INSIDE: u16 = 2;

/// The rows that stand on every screen whatever it holds: the trail, the rule, the row over
/// the legend and the legend.
const ALWAYS: u16 = 4;

/// The rows a box never covers: the trail and the rule above it, and under it the meter's
/// row, the row over the legend and the legend, which says the box's own keys while it is
/// open, so what the run spent and what needs the person stay in sight under any box.
const BOX_LEAVES: u16 = 5;

/// The fewest rows the regions keep before what the screen is about gives way: a run's pane
/// at the least terminal, its title, the row that says what its node works on, its feed's
/// heading and the three rows a feed's window keeps at the least, its count and its two
/// newest rows.
const LEAST_REGIONS: u16 = 6;

/// The fewest columns the application lays a screen out on: every screen is checked laid
/// out at 60 by 14, so a narrower terminal shows the room it needs rather than a screen
/// nobody checked.
pub(crate) const LEAST_COLUMNS: u16 = 60;

/// The fewest rows the application lays a screen out on: every screen is checked laid out
/// at 60 by 14, so a shorter terminal shows the room it needs rather than a screen nobody
/// checked.
pub(crate) const LEAST_ROWS: u16 = 14;

/// What a screen holds that takes room on a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Holds {
    /// The rows what the screen is about takes whole at the inner width; none for nothing
    /// about.
    pub(crate) about: u16,
    /// The region beside the main one, where there is one.
    pub(crate) side: Option<Beside>,
    /// Whether the main region has a title, which takes the first row of its column.
    pub(crate) main_titled: bool,
    /// Whether the screen says what its run spent, which takes the row over the one over the
    /// legend.
    pub(crate) meter: bool,
    /// The box of what is open over the screen, where something is.
    pub(crate) overlay: Option<Boxed>,
}

/// A region beside the main one: the edge of the screen it stands at, the cells of the edge
/// between them, and whether it has a title, which takes the first row of its column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Beside {
    pub(crate) at: SideAt,
    pub(crate) edge: u16,
    pub(crate) titled: bool,
}

/// What the box of what is open over a screen needs: the cells it stands wide, its edges
/// among them, and the rows it takes, its edges with what it holds. Not where it stands,
/// which the layout says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Boxed {
    pub(crate) columns: u16,
    pub(crate) rows: u16,
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
    pub(crate) meter: Option<Rect>,
    pub(crate) attention: Rect,
    pub(crate) legend: Rect,
    pub(crate) boxed: Option<Rect>,
    /// Whether the screen stands short of rows: what stands across it gave way its blank row
    /// so the regions keep their fewest, as on the least terminal, or, with nothing across
    /// it, it stands on the least terminal's rows. The meter then says its figures alone, its
    /// bars given way with the gap wherever they would fit, and a page in the main region
    /// lets its blank rows go.
    pub(crate) short_of_rows: bool,
}

/// Where each part of a screen holding `holds` stands on a terminal of `size`; none under
/// the least terminal, where the screen is the room it needs.
pub(crate) fn areas(size: Rect, holds: Holds) -> Option<Areas> {
    if size.width < LEAST_COLUMNS || size.height < LEAST_ROWS {
        return None;
    }
    let side = holds.side.filter(|_| size.width >= SIDE_FROM);
    let rows = Rows::of(size.height, holds);
    let [trail, rule, about, _, regions, meter, attention, legend] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(rows.about),
        Constraint::Length(u16::from(rows.blank)),
        Constraint::Length(rows.regions),
        Constraint::Length(u16::from(holds.meter)),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(size);
    let inside = |area: Rect| area.inner(ratatui::layout::Margin::new(MARGIN, 0));
    let (beside_main, main) = beside(inside(regions), side);
    let titled = |column: Rect, titled: bool| match titled {
        true => {
            let [title, under] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(column);
            (Some(title), under)
        }
        false => (None, column),
    };
    let (main_title, main) = titled(main, holds.main_titled);
    let side_titled = side.is_some_and(|side| side.titled);
    let side_areas = beside_main.map(|(column, _)| titled(column, side_titled));
    Some(Areas {
        trail: inside(trail),
        rule,
        about: (rows.about > 0).then(|| inside(about)),
        side_title: side_areas.and_then(|(title, _)| title),
        side: side_areas.map(|(_, side)| side),
        edge: beside_main.map(|(_, edge)| edge),
        main_title,
        main,
        meter: holds.meter.then(|| inside(meter)),
        attention: inside(attention),
        legend: inside(legend),
        boxed: holds.overlay.map(|needs| boxed(size, needs)),
        short_of_rows: match holds.about {
            0 => size.height <= LEAST_ROWS,
            _ => !rows.blank,
        },
    })
}

/// The rows what the screen is about, the blank row under it and the regions take on a
/// terminal `height` rows high: short of rows, the blank row gives way first, then what the
/// screen is about from its whole, a row at a time, while the regions would keep fewer than
/// [`LEAST_REGIONS`]; the regions take every row left.
struct Rows {
    about: u16,
    blank: bool,
    regions: u16,
}

impl Rows {
    fn of(height: u16, holds: Holds) -> Self {
        let left = height.saturating_sub(ALWAYS + u16::from(holds.meter));
        let mut rows = Rows {
            about: holds.about,
            blank: holds.about > 0,
            regions: 0,
        };
        loop {
            let across = rows.about + u16::from(rows.blank);
            rows.regions = left.saturating_sub(across);
            if rows.regions >= LEAST_REGIONS || across == 0 {
                return rows;
            }
            match rows.blank {
                true => rows.blank = false,
                false => rows.about -= 1,
            }
        }
    }
}

/// `area` shared between the side region `side`, where one stands, with the edge between
/// it and the main region, and the main region, which takes what they leave.
fn beside(area: Rect, side: Option<Beside>) -> (Option<(Rect, Rect)>, Rect) {
    let Some(side) = side else {
        return (None, area);
    };
    let edge = Constraint::Length(side.edge);
    let main = Constraint::Fill(1);
    match side.at {
        SideAt::Left => {
            let [side, edge, main] =
                Layout::horizontal([Constraint::Length(SIDE_AT_LEFT), edge, main]).areas(area);
            (Some((side, edge)), main)
        }
        SideAt::Right => {
            let [main, edge, side] =
                Layout::horizontal([main, edge, Constraint::Length(SIDE_AT_RIGHT)]).areas(area);
            (Some((side, edge)), main)
        }
    }
}

/// The box what is open over a terminal of `size` stands in, as `needs` says where it
/// fits: as wide as it needs up to [`box_columns`], centred; between the rule and the
/// meter's row, its top a third of the way down the rows it leaves and never above the row
/// under the rule.
fn boxed(size: Rect, needs: Boxed) -> Rect {
    let columns = needs.columns.min(box_columns(size.width));
    let rows = needs.rows.min(size.height.saturating_sub(BOX_LEAVES));
    let top = ((size.height - rows) / 3).max(2);
    Rect::new(
        size.x + (size.width - columns) / 2,
        size.y + top,
        columns,
        rows,
    )
}

/// The cells inside the margins of a terminal `columns` wide: what each screen says is
/// laid out at that width.
pub(crate) fn inner_columns(columns: u16) -> u16 {
    columns.saturating_sub(2 * MARGIN)
}

/// The most cells a box stands in on a terminal `columns` wide: [`BOX_MOST`], and
/// [`BOX_INSIDE`] inside each edge. What is open over it is measured at that width, before
/// the box's own width and rows are known.
pub(crate) fn box_columns(columns: u16) -> u16 {
    BOX_MOST.min(columns.saturating_sub(2 * BOX_INSIDE))
}

#[cfg(test)]
mod tests;

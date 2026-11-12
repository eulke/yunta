//! A Yunta screen drawn into a Ratatui frame: where each part stands is `layout`'s, what
//! each draws is Surco's, at its area's exact width, and its lines are written into the
//! frame's cells by `cells`. The cursor stands where a held prompt puts it, the box's over
//! a region's, and nowhere else. Under the least terminal the screen is the room it needs,
//! centred as a block.

use ratatui::layout::{Position, Rect};
use ratatui::Frame;
use surco::components::{Screen, ScreenLaid};
use surco::line::Line;
use surco::look::{Look, Width};
use surco::surface::{Paned, Terminal};
use surco::width::cell_width;

use crate::cells;
use crate::layout::{self, Areas, Beside, Holds, LEAST_COLUMNS, LEAST_ROWS};

/// `screen` drawn into `frame` in `look`'s glyphs, paint and motion, each part at the
/// width of the area it stands in; what was laid out of each region and of what is open
/// over them, for the next key to scroll from.
pub fn drawn(frame: &mut Frame, screen: &Screen, look: &Look) -> ScreenLaid {
    let Some(areas) = layout::areas(frame.area(), holds(screen, look, frame.area())) else {
        room_needed(frame, look);
        return ScreenLaid::default();
    };
    around(frame, screen, look, &areas);
    let main = terminal(look, areas.main.width).pane(&screen.main, usize::from(areas.main.height));
    write(frame, &main.lines, areas.main, look);
    let side = screen.side.as_ref().zip(areas.side).map(|(side, at)| {
        let paned = terminal(look, at.width).pane(&side.region, usize::from(at.height));
        write(frame, &paned.lines, at, look);
        (paned, at)
    });
    let boxed = screen
        .overlay
        .as_ref()
        .zip(areas.boxed)
        .map(|(overlay, at)| {
            let boxed = terminal(look, at.width).boxed(overlay, usize::from(at.height));
            write(frame, &boxed.lines, at, look);
            (boxed, at)
        });
    let held = boxed
        .as_ref()
        .or(side.as_ref().filter(|(paned, _)| paned.cursor.is_some()))
        .map_or((&main, areas.main), |(paned, at)| (paned, *at));
    placed_cursor(frame, held);
    ScreenLaid {
        main: main.laid,
        side: side.map(|(paned, _)| paned.laid),
        overlay: boxed.map(|(boxed, _)| boxed.laid),
    }
}

/// What `screen` holds that takes room on a terminal of `area`, measured from Surco's
/// lines in `look`.
fn holds(screen: &Screen, look: &Look, area: Rect) -> Holds {
    let inner = layout::inner_columns(area.width);
    let about = screen
        .about
        .as_ref()
        .map_or(0, |about| terminal(look, inner).lines(about).len());
    let edge = cell_width(&terminal(look, area.width).edge().text());
    let overlay = screen.overlay.as_ref().map(|overlay| {
        let columns = layout::box_columns(area.width);
        let whole = terminal(look, columns).boxed(overlay, usize::MAX);
        cells_or_rows(whole.lines.len())
    });
    Holds {
        about: cells_or_rows(about),
        side: screen.side.as_ref().map(|side| Beside {
            edge: cells_or_rows(edge),
            titled: side.region.title.is_some(),
        }),
        main_titled: screen.main.title.is_some(),
        attention: screen.attention.is_some(),
        overlay,
    }
}

/// The rows around the regions: the trail, the rule, what the screen is about, the
/// regions' titles and the edge between them, the attention and the legend, which is the
/// box's where a box is open.
fn around(frame: &mut Frame, screen: &Screen, look: &Look, areas: &Areas) {
    let at = |area: Rect| terminal(look, area.width);
    write(
        frame,
        &[at(areas.trail).trail(&screen.trail, &screen.tally)],
        areas.trail,
        look,
    );
    write(frame, &[at(areas.rule).rule()], areas.rule, look);
    if let Some((about, area)) = screen.about.as_ref().zip(areas.about) {
        write(frame, &at(area).lines(about), area, look);
    }
    let side_focused = screen.side.as_ref().is_some_and(|side| side.focused);
    if let Some((title, area)) = screen.main.title.as_ref().zip(areas.main_title) {
        write(frame, &[at(area).title(title, !side_focused)], area, look);
    }
    let side_title = screen
        .side
        .as_ref()
        .and_then(|side| side.region.title.as_ref());
    if let Some((title, area)) = side_title.zip(areas.side_title) {
        write(frame, &[at(area).title(title, side_focused)], area, look);
    }
    if let Some(area) = areas.edge {
        let edge = at(area).edge();
        let rows: Vec<Line> = (0..area.height).map(|_| edge.clone()).collect();
        write(frame, &rows, area, look);
    }
    if let Some((attention, area)) = screen.attention.as_ref().zip(areas.attention) {
        write(frame, &[at(area).attention(attention)], area, look);
    }
    let legend = screen
        .overlay
        .as_ref()
        .map_or(&screen.legend, |overlay| &overlay.legend);
    write(
        frame,
        &[at(areas.legend).legend(legend)],
        areas.legend,
        look,
    );
}

/// The cursor where the prompt `held`, drawn in its area, puts it; hidden where it puts
/// none.
fn placed_cursor(frame: &mut Frame, (held, at): (&Paned, Rect)) {
    if let Some((row, cell)) = held.cursor {
        let position = Position::new(at.x + cells_or_rows(cell), at.y + cells_or_rows(row));
        frame.set_cursor_position(position);
    }
}

/// What a terminal too small for the application shows: the room it needs, in render's
/// words through the interface, centred as a block.
fn room_needed(frame: &mut Frame, look: &Look) {
    let area = frame.area();
    let said = yunta_ui::room_needed(
        (usize::from(LEAST_COLUMNS), usize::from(LEAST_ROWS)),
        (usize::from(area.width), usize::from(area.height)),
    );
    // Inside the margins every screen keeps.
    let lines = terminal(look, layout::inner_columns(area.width)).lines(&said);
    let widest = lines.iter().map(|line| cell_width(&line.text())).max();
    let left = area.x
        + area
            .width
            .saturating_sub(cells_or_rows(widest.unwrap_or(0)))
            / 2;
    let top = area.y + area.height.saturating_sub(cells_or_rows(lines.len())) / 2;
    let block = Rect::new(
        left,
        top,
        area.right().saturating_sub(left),
        cells_or_rows(lines.len()),
    );
    write(frame, &lines, block.intersection(area), look);
}

/// Surco's terminal in `look`, its lines exactly `cells` wide.
fn terminal(look: &Look, cells: u16) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(usize::from(cells)),
        ..*look
    })
}

/// `lines` written into `frame`'s cells from `area`'s first row, one a row, as many as it
/// has rows.
fn write(frame: &mut Frame, lines: &[Line], area: Rect, look: &Look) {
    for (row, line) in lines.iter().take(usize::from(area.height)).enumerate() {
        let place = Rect::new(area.x, area.y + cells_or_rows(row), area.width, 1);
        cells::write(line, place, frame.buffer_mut(), look.paint);
    }
}

/// A count of cells or rows, as a terminal counts them: none passes what a terminal holds.
fn cells_or_rows(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

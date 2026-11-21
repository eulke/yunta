//! A Yunta screen drawn into a Ratatui frame: where each part stands is `layout`'s, what
//! each draws is Surco's, at its area's exact width, and its lines are written into the
//! frame's cells by `cells`. The main region's title row is the region's where Surco says
//! a card there cannot stand under it. What the screen is about is drawn as a band in the rows the
//! layout leaves it, Surco letting go what gives way first. The row over the legend holds
//! the note where one stands, the attention otherwise, and stays blank with neither. A box
//! stands as wide as the widest row Surco draws in it at the most a box stands and two cells
//! past it, a row on the selection reaching its far edge. The cursor stands where a held prompt puts it, the box's
//! over a region's, and nowhere else. Under the least terminal the screen is the room it
//! needs, centred as a block.

use ratatui::layout::{Position, Rect};
use ratatui::Frame;
use surco::components::{Legend, Screen, ScreenLaid};
use surco::line::Line;
use surco::look::{Look, Width};
use surco::surface::{Paned, Terminal};
use surco::width::cell_width;

use crate::cells;
use crate::layout::{self, Areas, Beside, Boxed, Holds, LEAST_COLUMNS, LEAST_ROWS};

/// `screen` drawn into `frame` in `look`'s glyphs, paint and motion, each part at the
/// width of the area it stands in; what was laid out of each region and of what is open
/// over them, for the next key to scroll from.
pub fn drawn(frame: &mut Frame, screen: &Screen, look: &Look) -> ScreenLaid {
    let Some(mut areas) = layout::areas(frame.area(), holds(screen, look, frame.area())) else {
        room_needed(frame, look);
        return ScreenLaid::default();
    };
    let on_main = terminal(look, areas.main.width);
    let title_given_way = title_given_way(screen, &on_main, &mut areas);
    around(frame, screen, look, &areas);
    let main_rows = usize::from(areas.main.height);
    let main = match (title_given_way, areas.short_of_rows) {
        (Some(_), short_of_rows) => {
            on_main.pane_in_place_of_its_title(&screen.main, main_rows, short_of_rows)
        }
        (None, true) => on_main.pane_short_of_rows(&screen.main, main_rows),
        (None, false) => on_main.pane(&screen.main, main_rows),
    };
    write(frame, &main.lines, areas.main, look);
    let side = screen.side.as_ref().zip(areas.side).map(|(side, at)| {
        let paned = terminal(look, at.width).pane(&side.region, usize::from(at.height));
        write(frame, &paned.lines, at, look);
        (paned, at)
    });
    let over = terminal(look, areas.legend.width);
    let boxed = screen
        .overlay
        .as_ref()
        .zip(areas.boxed)
        .map(|(overlay, at)| {
            let boxed = over.boxed(overlay, usize::from(at.width), usize::from(at.height));
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

/// The row of the main region's title where a card whose fewest rows do not stand under it
/// takes that row, as `on_main` draws the main region: given to the region in `areas`, and
/// none where the title keeps its row.
fn title_given_way(screen: &Screen, on_main: &Terminal, areas: &mut Areas) -> Option<Rect> {
    let rows = usize::from(areas.main.height);
    let title = areas
        .main_title
        .filter(|_| !on_main.keeps_its_title(&screen.main, rows))?;
    areas.main_title = None;
    areas.main = title.union(areas.main);
    Some(title)
}

/// What `screen` holds that takes room on a terminal of `area`, measured from Surco's
/// lines in `look`: what it is about drawn whole, and the box of what is open over it drawn
/// at the most a box stands.
fn holds(screen: &Screen, look: &Look, area: Rect) -> Holds {
    let inner = layout::inner_columns(area.width);
    let about = screen.about.as_ref().map_or(0, |about| {
        terminal(look, inner).across(about, usize::MAX).len()
    });
    let edge = cell_width(&terminal(look, area.width).edge().text());
    let overlay = screen.overlay.as_ref().map(|overlay| {
        let most = layout::box_columns(area.width);
        let whole = terminal(look, inner).boxed(overlay, usize::from(most), usize::MAX);
        Boxed {
            columns: cells_or_rows(box_needs(&whole.lines, &overlay.title, most)),
            rows: cells_or_rows(whole.lines.len()),
        }
    });
    Holds {
        about: cells_or_rows(about),
        side: screen.side.as_ref().map(|side| Beside {
            at: side.at,
            edge: cells_or_rows(edge),
            titled: !side.region.title.is_empty(),
        }),
        main_titled: !screen.main.title.is_empty(),
        meter: screen.meter.is_some(),
        overlay,
    }
}

/// The cells around a box's title in its top edge: a corner and a cell before it, a cell
/// and a corner after it.
const AROUND_A_TITLE: usize = 4;

/// The blank cells a box as wide as what it holds keeps past its widest row, before its far
/// upright: two, as the design's help leaves them past its longest row and a confirmation
/// past its sentence.
const PAST_THE_WIDEST: usize = 2;

/// The cells a box needs whose rows Surco drew `most` cells wide as `whole`, under `title`:
/// its widest row inside its edges, the trailing blanks that filled it out to the far
/// upright left out, two blank cells past it and that upright; a row on the selection, a
/// list a person picks from, reaching the far upright; its top edge, the title with the
/// cells around it.
fn box_needs(whole: &[Line], title: &str, most: u16) -> usize {
    let most = usize::from(most);
    let inside = whole
        .get(1..whole.len().saturating_sub(1))
        .unwrap_or_default();
    let needs = |row: &Line| match row.spans().iter().any(|span| span.fill.is_some()) {
        true => most,
        false => {
            let text = row.text();
            let mut cells = text.chars();
            let far_upright = cells
                .next_back()
                .map_or(0, |upright| cell_width(&upright.to_string()));
            cell_width(cells.as_str().trim_end()) + PAST_THE_WIDEST + far_upright
        }
    };
    let widest = inside.iter().map(needs).max().unwrap_or(0);
    widest.max(cell_width(title) + AROUND_A_TITLE).min(most)
}

/// The rows around the regions: the trail, the rule, what the screen is about, the
/// regions' titles and the edge between them, and the rows under them.
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
        let band = at(area).across(about, usize::from(area.height));
        write(frame, &band, area, look);
    }
    let side_focused = screen.side.as_ref().is_some_and(|side| side.focused);
    let titles = [
        areas
            .main_title
            .map(|area| (&screen.main, !side_focused, area)),
        (screen.side.as_ref().zip(areas.side_title))
            .map(|(side, area)| (&side.region, side_focused, area)),
    ];
    for (region, focused, area) in titles.into_iter().flatten() {
        write(frame, &[at(area).title(region, focused)], area, look);
    }
    if let Some(area) = areas.edge {
        let edge = at(area).edge();
        let rows: Vec<Line> = (0..area.height).map(|_| edge.clone()).collect();
        write(frame, &rows, area, look);
    }
    under_the_regions(frame, screen, look, areas);
}

/// The rows under the regions: what the run spent, in figures alone where the screen stands
/// short of rows, the note in the attention's place or the attention, and the keys: the
/// screen's with the far keys, the screen's alone where a line is typed on it, or the box's
/// where a box is open.
fn under_the_regions(frame: &mut Frame, screen: &Screen, look: &Look, areas: &Areas) {
    let at = |area: Rect| terminal(look, area.width);
    if let Some((meter, area)) = screen.meter.as_ref().zip(areas.meter) {
        let row = match areas.short_of_rows {
            true => at(area).meter_figures(meter),
            false => at(area).meter(meter),
        };
        write(frame, &[row], area, look);
    }
    let area = areas.attention;
    let over_the_legend = match (&screen.noted, &screen.attention) {
        (Some(noted), _) => Some(at(area).noted(noted)),
        (None, Some(attention)) => Some(at(area).attention(attention)),
        (None, None) => None,
    };
    write(frame, over_the_legend.as_slice(), area, look);
    // A box's own keys stand for the screen's while it is open, with no far keys beside them;
    // a screen where a line is typed says only the keys that act there.
    let on_the_legend = at(areas.legend);
    let keys = match (&screen.overlay, screen.typed) {
        (Some(overlay), _) => on_the_legend.legend(&overlay.legend, &Legend::new()),
        (None, true) => on_the_legend.typed_legend(&screen.legend),
        (None, false) => on_the_legend.legend(&screen.legend, &screen.anywhere),
    };
    write(frame, &[keys], areas.legend, look);
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

//! The terminal the walk shows each state's screen on, a few rows to a region so every
//! page a stage shows scrolls: what it lays out of a screen, handed back as a surface
//! hands it back, and where a page it shows stands scrolled.

use surco::components::{Screen, ScreenLaid, Sight};
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_ui::State;

/// The rows the walk's terminal gives each region: fewer than any page a stage shows
/// takes, so each scrolls.
const ROWS: usize = 4;

/// The cells the walk's terminal is wide: a terminal a person reads a page on.
const COLUMNS: usize = 80;

/// The walk's terminal: [`COLUMNS`] cells, in Unicode glyphs, unpainted.
fn terminal_look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(COLUMNS),
        motion: Motion::Still,
    }
}

/// What the walk's terminal lays out of `state`'s screen, each region and what is open
/// over them in [`ROWS`] rows:
/// what a surface hands back after it draws.
pub fn laid_out(state: &State) -> ScreenLaid {
    let screen = state.screen(&terminal_look());
    let terminal = Terminal::on(terminal_look());
    let main = terminal
        .scrolled(&screen.main.doc, screen.main.sight, ROWS)
        .1;
    let side = screen.side.as_ref().map(|side| {
        terminal
            .scrolled(&side.region.doc, side.region.sight, ROWS)
            .1
    });
    let overlay = screen
        .overlay
        .as_ref()
        .map(|overlay| terminal.scrolled(&overlay.doc, overlay.sight, ROWS).1);
    ScreenLaid {
        main,
        side,
        overlay,
    }
}

/// Where the page `screen` shows scrolled past its first line stands: `beside` for the
/// page beside a run's nodes, the last place on the trail for a page in the main region;
/// none where nothing was scrolled, or where the main region reads at its end.
pub fn scrolled(screen: &Screen) -> Option<&str> {
    let beside = screen
        .side
        .as_ref()
        .is_some_and(|side| side.region.sight.top > 0);
    let main = screen.main.sight != Sight::at_the_end() && screen.main.sight.top > 0;
    match (beside, main) {
        (true, _) => Some("beside"),
        (false, true) => screen.trail.last().map(String::as_str),
        (false, false) => None,
    }
}

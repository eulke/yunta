//! The terminal the walk shows each state's screen on, a few rows to a region so every
//! page a stage shows scrolls: what it lays out of a screen, handed back as a surface
//! hands it back, and where a page it shows stands scrolled.

use surco::components::ScreenLaid;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_ui::State;

/// The rows the walk's terminal gives each region, and what is open over them inside its
/// box: fewer than any page a stage shows takes, so each scrolls.
const ROWS: usize = 4;

/// The rows a box's edges take around what it holds.
const BOX_EDGES: usize = 2;

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

/// What the walk's terminal lays out of `state`'s screen, each region in a pane of
/// [`ROWS`] rows, as every surface draws a region, and what is open over them boxed with as
/// many inside: what a surface hands back after it draws.
pub(crate) fn laid_out(state: &State) -> ScreenLaid {
    let screen = state.screen(&terminal_look());
    let terminal = Terminal::on(terminal_look());
    let main = terminal.pane(&screen.main, ROWS).laid;
    let side = screen
        .side
        .as_ref()
        .map(|side| terminal.pane(&side.region, ROWS).laid);
    let overlay = screen
        .overlay
        .as_ref()
        .map(|overlay| terminal.boxed(overlay, ROWS + BOX_EDGES).laid);
    ScreenLaid {
        main,
        side,
        overlay,
    }
}

//! See [`super`]. Outside a repository: render's signpost on screen, which says yunta works
//! in one, under the trail alone, since nothing is followed here; `e` reaches every run on
//! this machine, the field at that reach, which the application then follows, and the
//! palette offers it first; `q`, Escape or Ctrl-C leaves.

use surco::components::{Region, Screen};
use surco::look::Look;
use yunta_core::step::{NextStep, Reach};
use yunta_render::page::Signpost;

use super::{Act, Want};
use crate::prompt::Stroke;

/// What `e` reaches from the signpost: the runs on this machine.
const EVERY_RUN_ON_THIS_MACHINE: NextStep = NextStep::ListRuns {
    reach: Reach::Machine,
};

/// The signpost, under the trail every screen starts at, with its keys and the keys every
/// stage answers to beside them.
pub(super) fn screen(_look: &Look) -> Screen {
    let mut screen = Screen::new(Region::of_a_page(Signpost.on_screen()));
    screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
    screen.legend = yunta_render::screen::signpost_legend();
    screen.anywhere = yunta_render::screen::anywhere();
    screen
}

/// What the signpost offers, with the key that takes it here: the runs on this machine.
pub(super) fn offered() -> Vec<Act> {
    vec![Act::by(Stroke::Insert('e'), EVERY_RUN_ON_THIS_MACHINE)]
}

/// What a stroke asks for at the signpost: the runs on this machine by `e`; leaving, by
/// `q`, Escape or Ctrl-C.
pub(super) fn pressed(stroke: Stroke) -> Vec<Want> {
    match stroke {
        Stroke::Insert('e') => Want::taking(&EVERY_RUN_ON_THIS_MACHINE),
        Stroke::Insert('q') | Stroke::Decline | Stroke::Interrupt => vec![Want::Leave],
        _ => Vec::new(),
    }
}

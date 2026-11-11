//! See [`super`]. Outside a repository: render's signpost, which says yunta works in
//! one and the ways on from here, and nothing to do but leave.

use surco::components::{Region, Screen};
use surco::look::Look;
use yunta_render::page::{Page, Signpost};

use super::Want;
use crate::prompt::Stroke;

/// The signpost, under the trail every screen starts at.
pub(super) fn screen(look: &Look) -> Screen {
    let mut screen = Screen::new(Region::new(Signpost.document(look)));
    screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
    screen.legend = yunta_render::screen::leaving_legend();
    screen
}

/// What a stroke asks for at the signpost: leaving, by `q`, Escape or Ctrl-C.
pub(super) fn pressed(stroke: Stroke) -> Vec<Want> {
    match stroke {
        Stroke::Insert('q') | Stroke::Decline | Stroke::Interrupt => vec![Want::Leave],
        _ => Vec::new(),
    }
}

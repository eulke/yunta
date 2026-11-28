//! See [`crate`]. What a surface shows where it has less room than it lays a screen out in:
//! the room it needs, in render's words, so a surface that writes no sentence shows it too;
//! and whether it has that room ([`Room`]), which a surface hands in as it changes.

use surco::components::Prose;
use surco::doc::Doc;

/// The document a surface shows, alone, where it has less room than it lays a screen out
/// in: `least` is the room it needs, `has` the room it has, each as columns by rows.
pub fn room_needed(least: (usize, usize), has: (usize, usize)) -> Doc {
    Doc::new().with(Prose(yunta_render::screen::room_needed(least, has)))
}

/// Whether a surface has the room it lays a screen out in. Where it is short of it, it shows
/// the room it needs alone ([`room_needed`]), which says only leaving: `q` and Ctrl-C leave,
/// and no key reaches the stage under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Room {
    #[default]
    Enough,
    Short,
}

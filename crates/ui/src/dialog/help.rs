//! See [`super`]. The keys of where the person is: each key the screen under it answers
//! to, with what it does, then those every stage answers to, worded by render. It is
//! built from the legend of the screen it stands over, so help never says a key the
//! screen does not take. Taller than its box, it scrolls; Escape or `?` closes it.

use surco::components::{Laid, Legend, Overlay};
use surco::look::Look;

use crate::prompt::Stroke;
use crate::scroll::Scroll;

/// The key that opens help over a stage where nothing is being typed, and closes it.
pub(crate) const OPENS: Stroke = Stroke::Insert('?');

/// The keys of where the person is, and where help is scrolled to.
#[derive(Debug, Clone, Default)]
pub struct Help {
    scroll: Scroll,
}

/// What a stroke at help came to.
pub(crate) enum Helped {
    /// Closed.
    Closed,
    /// The person leaves the application.
    Leave,
    /// It stays open, drawn again.
    Stay,
}

impl Help {
    /// Help as the overlay a surface draws over a screen whose keys are `keys`.
    pub(crate) fn overlay(&self, keys: &Legend, look: &Look) -> Overlay {
        let listed = yunta_render::screen::help(keys, look.glyphs);
        Overlay::new(yunta_render::screen::HELP, listed)
            .legend(yunta_render::screen::help_legend())
            .in_sight(self.scroll.sight())
    }

    /// What `stroke` does here: Escape or `?` closes it, Ctrl-C leaves, and the keys that
    /// scroll scroll it.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Helped {
        match stroke {
            Stroke::Decline | OPENS => Helped::Closed,
            Stroke::Interrupt => Helped::Leave,
            other => {
                self.scroll.pressed(other);
                Helped::Stay
            }
        }
    }

    /// What a surface laid out of it, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }
}

//! See [`super`]. An act asked to be confirmed before it is done: what it does, in
//! render's words, and the work it asks for once confirmed.

use surco::components::{Gist, Overlay};
use surco::doc::Doc;

use crate::prompt::Stroke;
use crate::Operation;

/// An act that waits on a person's confirmation: what it asks, and the work it is.
#[derive(Debug, Clone)]
pub struct Confirmation {
    asks: String,
    then: Operation,
}

/// What a stroke at a confirmation came to.
pub(crate) enum Confirmed {
    /// Confirmed: the work it asks for.
    Yes(Operation),
    /// Gone back from: nothing is done.
    No,
    /// The person stops the application.
    Leave,
    /// The stroke means nothing here.
    Stay,
}

impl Confirmation {
    /// The confirmation of `then`, asked as `asks`.
    pub(crate) fn new(asks: String, then: Operation) -> Self {
        Confirmation { asks, then }
    }

    /// The confirmation as the overlay a surface draws over the screen.
    pub(crate) fn overlay(&self) -> Overlay {
        Overlay {
            title: yunta_render::screen::CONFIRM.to_string(),
            doc: Doc::new().with(Gist::new(self.asks.clone())),
            legend: yunta_render::screen::confirmation_legend(),
        }
    }

    /// What `stroke` does here: Enter confirms, Escape goes back, Ctrl-C leaves.
    pub(crate) fn pressed(&self, stroke: Stroke) -> Confirmed {
        match stroke {
            Stroke::Enter => Confirmed::Yes(self.then.clone()),
            Stroke::Decline => Confirmed::No,
            Stroke::Interrupt => Confirmed::Leave,
            _ => Confirmed::Stay,
        }
    }
}

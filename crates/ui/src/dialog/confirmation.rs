//! See [`super`]. An act asked to be confirmed before it is done: what it does, in
//! render's words, the command that does the same, spelled by render's `command`, and
//! the work it asks for once confirmed.

use surco::components::{Gist, Next, Overlay};
use surco::doc::Doc;
use yunta_core::step::NextStep;
use yunta_render::command;

use crate::prompt::Stroke;
use crate::Operation;

/// An act that waits on a person's confirmation: what it asks, the commands that do the
/// same, and the work it is.
#[derive(Debug, Clone)]
pub struct Confirmation {
    asks: String,
    same: Vec<NextStep>,
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
    /// The confirmation of `then`, asked as `asks`, which the commands `same` do at a
    /// shell.
    pub(crate) fn new(asks: String, same: Vec<NextStep>, then: Operation) -> Self {
        Confirmation { asks, same, then }
    }

    /// The confirmation as the overlay a surface draws over the screen: what it asks,
    /// then the commands that do the same.
    pub(crate) fn overlay(&self) -> Overlay {
        let steps = self
            .same
            .iter()
            .map(|step| (command::line(step), yunta_render::screen::DOES_THE_SAME))
            .collect();
        let asks = Doc::new()
            .with(Gist::new(self.asks.clone()))
            .with(Next { steps });
        Overlay::new(yunta_render::screen::CONFIRM, asks)
            .legend(yunta_render::screen::confirmation_legend())
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

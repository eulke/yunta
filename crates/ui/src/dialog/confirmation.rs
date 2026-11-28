//! See [`super`]. An act asked to be confirmed before it is done: the act, which render
//! words ([`Confirming`]), the commands that do the same at a shell, spelled by render's
//! `command`, and the work it asks for once confirmed. Its box is titled with what it asks
//! and reads as a sentence under it: what the act does, then on a line each what it leaves
//! undone of what was asked (a run a close names and leaves open, with why), a blank row,
//! then the commands alone, each on its row; its legend names the act's own keys. It holds
//! all of it as it was asked, whatever moves under it until it is answered.
//!
//! Escape does nothing of what was asked, but where going back is an act of its own, which
//! the confirmation then holds beside the act and its legend names: a setup's pack declined
//! writes the setup without it.

use surco::components::{Overlay, SameAtAShell};
use surco::doc::{Block, Doc};
use surco::line::Line;
use yunta_core::step::NextStep;
use yunta_render::command;
use yunta_render::screen::Confirming;

use crate::prompt::Stroke;
use crate::Operation;

/// An act that waits on a person's confirmation: what it asks, the commands that do the
/// same, the work it is, and the work going back from it is, where it is any, on the heap as
/// few confirmations hold one.
#[derive(Debug, Clone)]
pub struct Confirmation {
    asks: Confirming,
    same: Vec<NextStep>,
    then: Operation,
    declined: Option<Box<Operation>>,
}

/// What a stroke at a confirmation came to.
pub(crate) enum Confirmed {
    /// Confirmed: the work it asks for.
    Yes(Operation),
    /// Gone back from: nothing is done, or the work going back from it is, where it is any.
    No(Option<Operation>),
    /// The person stops the application.
    Leave,
    /// The stroke means nothing here.
    Stay,
}

impl Confirmation {
    /// The confirmation of `then`, the act `asks`, which the commands `same` do at a
    /// shell.
    pub(crate) fn new(asks: Confirming, same: Vec<NextStep>, then: Operation) -> Self {
        Confirmation {
            asks,
            same,
            then,
            declined: None,
        }
    }

    /// This confirmation, going back from it asking `declined`: an act of its own, which its
    /// legend names.
    pub(crate) fn declined_as(self, declined: Operation) -> Self {
        Confirmation {
            declined: Some(Box::new(declined)),
            ..self
        }
    }

    /// The confirmation as the overlay a surface draws over the screen: titled with what it
    /// asks, what the act does, then the commands that do the same at a shell, alone, and
    /// the act's keys.
    pub(crate) fn overlay(&self) -> Overlay {
        let same = SameAtAShell(self.same.iter().map(command::line).collect());
        let said = std::iter::once(self.asks.sentence()).chain(self.asks.left_undone());
        let does = Doc::new()
            .with(Block::Lines(
                said.map(|said| Line::new().plain(said)).collect(),
            ))
            .with(same);
        Overlay::new(self.asks.question(), does)
            .legend(self.asks.legend())
            .confirming()
    }

    /// What `stroke` does here: Enter confirms, Escape goes back, asking what going back
    /// asks, Ctrl-C leaves.
    pub(crate) fn pressed(&self, stroke: Stroke) -> Confirmed {
        match stroke {
            Stroke::Enter => Confirmed::Yes(self.then.clone()),
            Stroke::Decline => Confirmed::No(self.declined.as_deref().cloned()),
            Stroke::Interrupt => Confirmed::Leave,
            _ => Confirmed::Stay,
        }
    }
}

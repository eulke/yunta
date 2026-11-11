//! See [`super`]. What a person can do from where they stand: the acts the stage offers,
//! then what every stage offers, each row the command that does the same, spelled by
//! render's `command`, with what it does, so a person learns the CLI from it. Typed
//! words narrow it. A row the stage offers is taken by the key it is bound to there, so
//! choosing it does what pressing the key does; a row every stage offers opens the stage
//! that takes it. Not the stage's legend, which says its keys: the palette says its acts.

use surco::components::{Overlay, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::step::NextStep;
use yunta_render::command;
use yunta_render::prompt::Escape;

use crate::prompt::{Alternative, Answerable, Picker, Press, Stroke};
use crate::stage::{Act, Stage};

/// The key that opens the palette over a stage where nothing is being typed.
pub(crate) const OPENS: Stroke = Stroke::Insert('/');

/// The acts a person can take from where they stand, narrowed by what they type.
#[derive(Debug, Clone)]
pub struct Palette {
    rows: Picker<Taken>,
}

/// How a row of the palette is taken.
#[derive(Debug, Clone)]
enum Taken {
    /// By the key it is bound to on the stage.
    ByKey(Stroke),
    /// By opening the stage that takes the step, one every stage offers.
    Opening(NextStep),
}

/// What a stroke at the palette came to.
pub(crate) enum Chosen {
    /// A row the stage offers: what pressing `0` there does.
    Pressed(Stroke),
    /// A row every stage offers: the stage that takes it, opened.
    Opened(Box<Stage>),
    /// Closed, with nothing done.
    Closed,
    /// The person leaves the application.
    Leave,
    /// It stays open, drawn again.
    Stay,
}

impl Palette {
    /// The palette over a stage that offers `acts`, then every step every stage offers
    /// that the stage neither offers nor is (`showing`).
    pub(crate) fn over(acts: Vec<Act>, showing: Option<NextStep>) -> Self {
        let always: Vec<NextStep> = Stage::always()
            .into_iter()
            .filter(|step| {
                Some(step) != showing.as_ref() && !acts.iter().any(|act| act.step == *step)
            })
            .collect();
        let offered = acts
            .into_iter()
            .map(|act| row(act.step, Taken::ByKey(act.key)));
        let rows = offered
            .chain(
                always
                    .into_iter()
                    .map(|step| row(step.clone(), Taken::Opening(step))),
            )
            .collect();
        Palette {
            rows: Picker::new(rows, Escape::GoesBack),
        }
    }

    /// The palette as the overlay a surface draws over the screen: the list, with its
    /// keys, the row pointed at in sight.
    pub(crate) fn overlay(&self, look: &Look) -> Overlay {
        let doc = Doc::new().with(self.rows.block(look));
        Overlay::new(yunta_render::screen::PALETTE, doc).in_sight(Sight::following_pointer())
    }

    /// What `stroke` does here: typing narrows the rows, the arrows move along them,
    /// Enter takes the one pointed at, Escape closes it and Ctrl-C leaves.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Chosen {
        match self.rows.press(stroke) {
            Press::Answer(Taken::ByKey(key)) => Chosen::Pressed(key),
            Press::Answer(Taken::Opening(step)) => match Stage::opened_by(&step) {
                Some(stage) => Chosen::Opened(Box::new(stage)),
                None => Chosen::Closed,
            },
            Press::Decline => Chosen::Closed,
            Press::Interrupt => Chosen::Leave,
            Press::Stay | Press::Moved => Chosen::Stay,
        }
    }
}

/// The row of `step`, taken as `taken`: its command, and what it does.
fn row(step: NextStep, taken: Taken) -> Alternative<Taken> {
    Alternative {
        name: command::line(&step),
        label: Some(command::does(&step).to_string()),
        value: taken,
    }
}

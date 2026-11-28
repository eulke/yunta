//! See [`super`]. What a person can do from where they stand: the acts the stage offers,
//! then what every stage offers, each row what it does, the key the stage takes it by where
//! one does, and the command that does the same, spelled by render's `command`, so a person
//! learns the keys and the CLI from it: what an act the stage offers does in the act's own
//! words, since two acts may share one command. Typed words narrow it by what a row does,
//! never by its command. A row the stage offers is taken by the key it is bound to there, so
//! choosing it does what pressing the key does; a row every stage offers, and one the stage
//! offers with no key bound to it there, is taken as its step is ([`Want::taking`]). Its own
//! keys stand in the screen's legend row while it is open, none inside its box. Not the
//! stage's legend, which says its keys: the palette says its acts.

use surco::components::{ChoiceList, Legend, Overlay, Sight};
use surco::doc::Doc;
use yunta_core::step::NextStep;
use yunta_render::command;
use yunta_render::prompt::{Escape, ENTER};

use crate::prompt::{Alternative, Answerable, Picker, Press, Stroke};
use crate::stage::{Act, Want};

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
    /// As its step is taken from any stage: one every stage offers, or one no key here takes.
    Stepping(NextStep),
}

/// What a stroke at the palette came to.
pub(crate) enum Chosen {
    /// A row the stage offers: what pressing `0` there does.
    Pressed(Stroke),
    /// A row taken as its step is: what taking it wants of the state, nothing where no
    /// stage takes it.
    Took(Vec<Want>),
    /// Closed, with nothing done.
    Closed,
    /// The person leaves the application.
    Leave,
    /// It stays open, drawn again.
    Stay,
}

impl Palette {
    /// The palette over a stage that offers `acts`, then each step of `always`, what every
    /// stage offers where the application stands, that the stage neither offers nor is
    /// (`showing`).
    pub(crate) fn over(acts: Vec<Act>, always: Vec<NextStep>, showing: Option<NextStep>) -> Self {
        let always: Vec<NextStep> = always
            .into_iter()
            .filter(|step| {
                Some(step) != showing.as_ref() && !acts.iter().any(|act| act.step == *step)
            })
            .collect();
        let offered = acts.into_iter().map(|act| {
            let taken = match act.key {
                Some(key) => Taken::ByKey(key),
                None => Taken::Stepping(act.step.clone()),
            };
            row(&act.step, act.does, act.key, taken)
        });
        let rows = offered
            .chain(always.into_iter().map(|step| {
                let does = command::does(&step);
                row(&step, does, None, Taken::Stepping(step.clone()))
            }))
            .collect();
        Palette {
            rows: Picker::new(rows, Escape::GoesBack),
        }
    }

    /// The palette as the overlay a surface draws over the screen: the list, the row
    /// pointed at in sight, with no keys of its own inside it, and its keys, which stand in
    /// the screen's legend row while it is open.
    pub(crate) fn overlay(&self) -> Overlay {
        let list = ChoiceList {
            choices: self.rows.list(),
            filter: self.rows.filter().map(str::to_string),
            legend: Legend::new(),
        };
        Overlay::new(yunta_render::screen::PALETTE, Doc::new().with(list))
            .legend(yunta_render::screen::palette_legend())
            .in_sight(Sight::following_pointer())
    }

    /// What `stroke` does here: typing narrows the rows, the arrows move along them,
    /// Enter takes the one pointed at, Escape closes it and Ctrl-C leaves.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Chosen {
        match self.rows.press(stroke) {
            Press::Answer(Taken::ByKey(key)) => Chosen::Pressed(key),
            Press::Answer(Taken::Stepping(step)) => Chosen::Took(Want::taking(&step)),
            Press::Decline => Chosen::Closed,
            Press::Interrupt => Chosen::Leave,
            Press::Stay | Press::Moved => Chosen::Stay,
        }
    }
}

/// The row of an act that takes `step`, does `does` and is bound to `key` here, where a key
/// is, taken as `taken`: named by what it does, which typing narrows it by; its key, as a
/// legend spells it; and its command.
fn row(step: &NextStep, does: &str, key: Option<Stroke>, taken: Taken) -> Alternative<Taken> {
    Alternative {
        label: Some(command::line(step)),
        key: key.and_then(spelled),
        ..Alternative::named(does, taken)
    }
}

/// `key` as a legend spells it in a row's key column: a character as it is typed, Enter by
/// its name. None for a stroke no act is bound to.
pub(crate) fn spelled(key: Stroke) -> Option<String> {
    match key {
        Stroke::Insert(character) => Some(character.to_string()),
        Stroke::Enter => Some(ENTER.to_string()),
        _ => None,
    }
}

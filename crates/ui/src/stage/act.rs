//! See [`super`]. An act a stage offers, as the palette lists it over the stage: the step it
//! takes, the key that takes it there, and what it does in the palette's words.

use yunta_core::step::NextStep;
use yunta_render::command;

use crate::prompt::Stroke;

/// An act a stage offers: the step it takes, as a command line names it, the key that takes
/// it there, where a key does, and what it does, as its palette row says it. Not the step
/// alone: the palette takes an act by its key, so a row chosen there does what pressing the
/// key does; an act no key here takes, the palette opens the stage that takes its step, as it
/// opens what every stage offers. Two acts may share one command and say what each does in
/// words of its own: the field follows a run with `enter`, and a run's palette opens its
/// whole chronicle, both what `yunta status <run> --follow` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Act {
    pub(crate) step: NextStep,
    pub(crate) key: Option<Stroke>,
    /// What it does, as its palette row says it: what its step does, or words of its own
    /// where another act shares its step's command.
    pub(crate) does: &'static str,
}

impl Act {
    /// `step`, taken here by `key`.
    pub(crate) fn by(key: Stroke, step: NextStep) -> Self {
        Act {
            does: command::does(&step),
            step,
            key: Some(key),
        }
    }

    /// `step`, which no key here takes: the key that takes it elsewhere does something
    /// else here.
    pub(crate) fn unbound(step: NextStep) -> Self {
        Act {
            does: command::does(&step),
            step,
            key: None,
        }
    }

    /// It said in `does`, words of its own, where another act shares its step's command and
    /// what that step does is the other act's to say.
    pub(crate) fn saying(self, does: &'static str) -> Self {
        Act { does, ..self }
    }
}

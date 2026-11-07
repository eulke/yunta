//! The furrow with the names of a run's nodes over it: the signature at the
//! top of a run, on a screen that stays open.

use crate::words::Standing;

/// The furrow with the names of the run's nodes over it: the signature at the
/// top of a run. Not a [`Furrow`](super::Furrow), which hangs under a page's
/// headline and names no node.
///
/// A surface draws each node's name over its stretch, in the tone of its word
/// where there is color, and the plow a cell further along the running node's
/// stretch for each event that node has had: only an event moves it. Where
/// the names do not fit, it is a page's furrow, the strokes alone, which the
/// words beside them say.
pub struct Ribbon {
    /// Each node's stretch, in the order the workflow declares them.
    pub stretches: Vec<Stretch>,
    /// How many events the running node has had: the plow stands a cell
    /// further along its stretch for each. `None` where the view stands
    /// still, and no plow is drawn.
    pub events: Option<u64>,
}

/// One node's stretch of the furrow: its name and the word it stands at.
pub struct Stretch {
    pub name: String,
    pub standing: Standing,
}

impl Ribbon {
    /// The ribbon of `stretches`, in the order the workflow declares them,
    /// with no plow.
    pub fn new(stretches: impl IntoIterator<Item = Stretch>) -> Self {
        Ribbon {
            stretches: stretches.into_iter().collect(),
            events: None,
        }
    }

    /// This ribbon with the plow where the running node's `events` put it.
    pub fn plowed(mut self, events: u64) -> Self {
        self.events = Some(events);
        self
    }
}

impl Stretch {
    pub fn new(name: impl Into<String>, standing: Standing) -> Self {
        Stretch {
            name: name.into(),
            standing,
        }
    }
}

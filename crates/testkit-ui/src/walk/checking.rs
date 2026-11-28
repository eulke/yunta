//! See [`super`]. What the walk's checks hold while the cores expand the states after the
//! ones they hold: what the states the walk expanded last came to ([`Unchecked`]), handed
//! to each check on a thread of its own, every input and every state kept in the order the
//! walk took them, so each check meets them as a walk of one input at a time would.

use std::panic::resume_unwind;

use yunta_ui::{Request, State};

use super::{Checks, Move, Walk};

/// What the states the walk expanded last came to, which the checks hold while the cores
/// expand the states after them: the states expanded, each at its place among the steps,
/// and each input handed to one of them, in the order the walk took them.
#[derive(Default)]
pub(super) struct Unchecked {
    pub(super) expanded: Vec<(usize, State)>,
    pub(super) handed: Vec<HandedOn>,
}

/// A move made on one of the states expanded, `to` by where it stands among them, what it
/// asked, and the state it came to where the walk kept it.
pub(super) struct HandedOn {
    pub(super) to: usize,
    pub(super) moved: Move,
    pub(super) asked: Vec<Request>,
    pub(super) kept: Option<Kept>,
}

/// A state the walk kept: its place among the steps, the state, and its screen as Markdown
/// draws it, which the checks read.
pub(super) struct Kept {
    pub(super) place: usize,
    pub(super) state: State,
    pub(super) markdown: String,
}

impl Walk<'_> {
    /// `unchecked` held by every one of `checks`, each on a thread of its own; what one of
    /// them found broken stops the walk with its own words.
    pub(super) fn checked(&self, unchecked: &Unchecked, checks: &mut [&mut dyn Checks]) {
        std::thread::scope(|scope| {
            let holding: Vec<_> = checks
                .iter_mut()
                .map(|checks| scope.spawn(move || self.checked_by(unchecked, &mut **checks)))
                .collect();
            for held in holding {
                held.join().unwrap_or_else(|panic| resume_unwind(panic));
            }
        });
    }

    /// `unchecked` held by `checks`: each input handed, then the state it came to where the
    /// walk kept it, in the order the walk took them. A palette's row taken is held as an
    /// input by the interface's check of the palette over the state it was taken on.
    fn checked_by(&self, unchecked: &Unchecked, checks: &mut dyn Checks) {
        for HandedOn {
            to,
            moved,
            asked,
            kept,
        } in &unchecked.handed
        {
            let (at, from) = &unchecked.expanded[*to];
            if let Move::Input(input) = moved {
                checks.handed(from, input, asked, &self.at(*at, moved));
            }
            if let Some(Kept {
                place,
                state,
                markdown,
            }) = kept
            {
                checks.reached(state, markdown, &self.reached_at(*place));
            }
        }
    }
}

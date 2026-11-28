//! See [`super`]. A run closed promoted, and the successor its promotion handed the work to,
//! which takes the screen while the run leads.
//!
//! As the successor comes, the state moves every stage that shows the run onto it where the
//! run leads its screen. Where the person holds it, the stage stays on the run, its ended
//! scene saying the successor kept beside it, until the lead goes back to the run, by `f`:
//! the successor takes the screen then, as it would have as it came.

use super::{Framed, RunStage};

impl RunStage {
    /// The stage moved onto the successor a promotion handed the work of the run `framed`
    /// to, where the run leads and its successor came. Nothing where the person holds the
    /// screen, or no successor came.
    pub(super) fn onto_its_successor(&mut self, framed: &Framed<'_>) {
        let successor = framed.in_view.successor.as_deref();
        let Some(successor) = successor.filter(|_| self.leads()) else {
            return;
        };
        let (run, by) = (self.run.clone(), successor.run.run().clone());
        self.succeeded(&run, &by);
    }
}

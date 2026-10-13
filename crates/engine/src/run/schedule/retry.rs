//! Whether a person's choice to run a failed node again still stands on the
//! menu its failure offers. A choice the menu no longer offers is asked about
//! again, not run.

use yunta_core::events::Outlook;
use yunta_core::NodeId;

use super::Board;

impl Board<'_> {
    /// Whether a person chose to run `id` again after its latest failure, and
    /// `outlook`, the menu that failure offers now, still offers that choice.
    pub(super) fn retry_chosen(&self, id: &NodeId, outlook: &Outlook) -> bool {
        let Outlook::Choose(remedies) = outlook else {
            return false;
        };
        self.state
            .run_again_chosen(id)
            .is_some_and(|option| remedies.iter().any(|remedy| remedy.option == option))
    }
}

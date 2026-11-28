//! What hangs from a page's headline: the run's furrow, and the page's
//! gist.

use crate::words::Standing;

/// Where each node of a run stands, in the order the workflow declares
/// them: the furrow a run plows, under its headline.
///
/// A surface draws it as it can: a stroke for each node, painted in the tone
/// of its word where there is color, and the same strokes where there is
/// none; with more nodes than room, a stroke stands for the nodes it covers
/// and takes the word that matters most among them. What it shows is said
/// in words beside it too — how many nodes are done — so a reader of the
/// plainest stream loses nothing.
pub struct Furrow(pub Vec<Standing>);

/// The gist of a page, in the sentence under its headline: what it asks
/// of its reader, what holds what it is about, or what was just done.
/// Said whole — wrapped, never cut — and ending with its period; or, on a
/// screen, as a run's gist is said, on its one row.
pub struct Gist {
    pub said: String,
    /// Whether it stands on its one row, as a run's gist does on a screen: cut
    /// where the row ends, with no closing period. Not wrapped whole, as a
    /// printed page says it.
    pub on_one_row: bool,
}

impl Gist {
    /// `said` as a page's gist: ended with a period when nothing ends it
    /// already.
    pub fn new(said: impl Into<String>) -> Self {
        let mut said: String = said.into();
        let ended = said.trim_end().ends_with(['.', '?', '!']);
        if !ended {
            said.truncate(said.trim_end().len());
            said.push('.');
        }
        Gist {
            said,
            on_one_row: false,
        }
    }

    /// This gist as a run's gist is said on a screen: on its one row, its
    /// closing period left out.
    pub fn on_one_row(self) -> Self {
        let said = self.said.strip_suffix('.').unwrap_or(&self.said);
        Gist {
            said: said.to_string(),
            on_one_row: true,
        }
    }
}

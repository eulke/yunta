//! What hangs from a page's headline: the run's furrow, and the page's
//! gist.

use crate::words::Standing;

/// Where each node of a run stands, in the order the workflow declares
/// them: the furrow a run plows, under its headline.
///
/// A surface draws it as it can: a stroke a node long for each node,
/// painted in the tone of its word where there is color, and the same
/// strokes where there is none. What it shows is said in
/// words beside it too — how many nodes are done — so a reader of the
/// plainest stream loses nothing.
pub struct Furrow(pub Vec<Standing>);

/// The gist of a page, in the sentence under its headline: what it asks
/// of its reader, what holds what it is about, or what was just done.
/// Said whole — wrapped, never cut — and ending with its period.
pub struct Gist(pub String);

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
        Gist(said)
    }
}

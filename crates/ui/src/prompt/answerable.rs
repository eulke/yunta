use surco::components::Laid;
use surco::doc::Block;
use surco::look::Look;

use super::{Press, Stroke};

/// A prompt a person answers stroke by stroke, drawn as one block on any surface.
/// Not a terminal's prompt loop, which holds one of these at its height: this is what
/// it holds.
pub trait Answerable {
    /// What answering it comes to.
    type Answer;

    /// What a stroke asks of it.
    fn press(&mut self, stroke: Stroke) -> Press<Self::Answer>;

    /// Whether a field is open, where what is pasted is what is typed. Anywhere else a
    /// paste answers nothing.
    fn typing(&self) -> bool;

    /// The prompt as it stands, as the block a surface draws, in the glyphs `look`
    /// draws with.
    fn block(&self, look: &Look) -> Block;

    /// What a surface laid out of a pane it scrolls, handed back after it drew it: a
    /// prompt that scrolls by what was shown reads it on the next stroke. A prompt
    /// with no pane has nothing to read.
    fn laid(&mut self, _laid: Laid) {}
}

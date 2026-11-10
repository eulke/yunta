//! A line asked of a person, answered on the line they type.

use surco::doc::Block;
use surco::line::Line;
use surco::look::Look;

use super::{Answerable, Edit, Press, Stroke, Typed, TypedLine};

/// A line being answered, under what asks for it.
pub struct LineAsked {
    asked: Line,
    line: TypedLine,
}

impl LineAsked {
    /// The line `asked` puts to a person, with nothing typed yet.
    pub fn new(asked: Line) -> Self {
        LineAsked {
            asked,
            line: TypedLine::default(),
        }
    }
}

impl Answerable for LineAsked {
    type Answer = Typed;

    /// The question, and the row the words are typed on, with the cursor on it.
    fn block(&self, _look: &Look) -> Block {
        let (before, after) = self.line.split();
        Block::TypedAnswer(yunta_render::prompt::typed_answer(
            &self.asked,
            before,
            after,
        ))
    }

    /// Enter answers with the line, whatever it holds; every other stroke edits it.
    fn press(&mut self, stroke: Stroke) -> Press<Typed> {
        match self.line.apply(stroke) {
            Edit::Editing => Press::Stay,
            Edit::Answered(typed) => Press::Answer(typed),
            Edit::Declined => Press::Decline,
            Edit::Interrupted => Press::Interrupt,
        }
    }

    /// Every key is a key of the line.
    fn typing(&self) -> bool {
        true
    }
}

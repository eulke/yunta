//! What a document is about, on its first line.

use crate::components::Headline;
use crate::line::{Line, Tone};
use crate::look::Look;

impl Headline {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let tone = Tone::of(self.mark);
        vec![Line::new()
            .push(Tone::Strong, self.subject.as_str())
            .plain(": ")
            .push(tone, look.glyphs.mark(self.mark).to_string())
            .plain(" ")
            .push(tone, self.said.as_str())]
    }
}

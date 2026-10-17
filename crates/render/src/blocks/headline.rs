//! The line a surface opens with: what it is about, and the word for
//! where that stands.

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::{Look, Mark};

/// What a surface is about — `run 7E5PH4` — and where it stands, marked.
pub struct Headline {
    pub subject: String,
    pub mark: Mark,
    pub said: String,
}

impl Drawn for Headline {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let tone = Tone::of(self.mark);
        vec![Line::new()
            .push(Tone::Strong, self.subject.as_str())
            .plain(": ")
            .push(tone, look.glyphs.mark(self.mark).to_string())
            .plain(" ")
            .push(tone, self.said.as_str())]
    }
}

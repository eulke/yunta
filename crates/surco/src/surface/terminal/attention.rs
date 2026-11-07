//! The attention on a terminal: what needs a person, and how they answer it,
//! on one row that keeps its answer when it is cut.

use crate::components::{Answer, Attention};
use crate::glyphs::Mark;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, truncate};
use crate::words::Standing;

impl Attention {
    /// One row: the mark and `needs you`, what needs the person when there is
    /// a subject, then the answer where a cut keeps it. A command stands
    /// before what is asked, which is said between parentheses, and the row is
    /// cut at its end; a key stands at the row's end, and what is asked is cut
    /// to leave it room; an attention answered elsewhere says only what is
    /// asked, cut at its end. The row carries no fill: a screen tints it where it
    /// stands. An attention that asks nothing draws nothing.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        if self.asks.trim().is_empty() {
            return Vec::new();
        }
        let glyphs = look.glyphs;
        let width = look.width.cells();
        let row = match &self.answer {
            Answer::Command(command) => self
                .head(look)
                .push(Tone::Strong, command.as_str())
                .push(Tone::Muted, format!(" ({})", self.asks)),
            Answer::Key(binding) => {
                let head = self.head(look);
                let tail = Line::new()
                    .push(Tone::Faint, format!(" {} ", glyphs.sep()))
                    .push(Tone::Strong, binding.keys.spelled(glyphs))
                    .push(Tone::Muted, format!(" {}", binding.does));
                let room =
                    width.saturating_sub(cell_width(&head.text()) + cell_width(&tail.text()));
                let asks = match cell_width(&self.asks) <= room {
                    true => self.asks.clone(),
                    false => truncate(&self.asks, room, glyphs),
                };
                head.plain(asks).then(tail)
            }
            Answer::Elsewhere => self.head(look).plain(self.asks.as_str()),
        };
        vec![row.cut(width, glyphs)]
    }

    /// The mark and `needs you`, with the subject between them when there is
    /// one, and the colon the answer follows.
    fn head(&self, look: &Look) -> Line {
        let mark = look.glyphs.mark(Mark::NeedsYou);
        let word = Standing::NeedsYou.word();
        let head = match &self.subject {
            None => Line::new().push(Tone::NeedsYou, format!("{mark} {word}")),
            Some(subject) => Line::new()
                .push(Tone::NeedsYou, mark.to_string())
                .plain(" ")
                .push(Tone::Strong, subject.as_str())
                .push(Tone::NeedsYou, format!(" {word}")),
        };
        head.plain(": ")
    }
}

#[cfg(test)]
mod tests;

//! What a document is about, on its first line.

use crate::components::Headline;
use crate::line::{Fill, Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, truncate};

impl Headline {
    /// The mark, the subject, the word and the facts it has room for, and
    /// the edge at the line's far edge. A narrow line drops the facts from
    /// the last, then cuts the subject; the word and the edge are never
    /// cut, since one says where it stands and the other is what a reader
    /// types next. The word is filled as a pill in the mark's tone: a layer
    /// drawing 24-bit color paints it as a badge, and every other layer draws
    /// the word alone, in the tone's color where that layer has one.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let glyphs = look.glyphs;
        let tone = Tone::of(self.mark);
        let sep = format!(" {} ", glyphs.sep());
        let mark = format!("{} ", glyphs.mark(self.mark));
        let edge = self.edge.as_deref().unwrap_or_default();
        let room = match edge.is_empty() {
            true => look.width.cells(),
            false => look.width.cells().saturating_sub(cell_width(edge) + 2),
        };
        let fixed = cell_width(&mark) + cell_width(&sep) + cell_width(&self.said);
        let subject = match cell_width(&self.subject) + fixed <= room {
            true => self.subject.clone(),
            false => truncate(&self.subject, room.saturating_sub(fixed), glyphs)
                .trim_end()
                .to_string(),
        };
        let mut used = fixed + cell_width(&subject);
        let mut line = Line::new()
            .push(tone, mark.trim_end())
            .plain(" ")
            .push(Tone::Strong, subject)
            .push(Tone::Muted, sep.as_str())
            .push_filled(tone, Fill::Pill, self.said.as_str());
        for fact in &self.facts {
            let takes = cell_width(&sep) + cell_width(fact);
            if used + takes > room {
                break;
            }
            used += takes;
            line = line.push(Tone::Muted, format!("{sep}{fact}"));
        }
        if !edge.is_empty() {
            let gap = look
                .width
                .cells()
                .saturating_sub(used + cell_width(edge))
                .max(2);
            line = line.plain(" ".repeat(gap)).plain(edge);
        }
        vec![line]
    }
}

#[cfg(test)]
mod tests {
    use crate::components::Headline;
    use crate::glyphs::Mark;
    use crate::line::{Fill, Tone};
    use crate::look::Look;

    /// The standing word is the one span of a headline that is filled, as a
    /// pill in its mark's tone. The words around it read the same, unfilled.
    #[test]
    fn the_standing_word_is_the_only_filled_span_of_a_headline() {
        let headline =
            Headline::new(Mark::NeedsYou, "fragua (standard)", "needs you").fact("for 32m10s");
        let lines = headline.lines(&Look::plain());
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].text(),
            "? fragua (standard) ; needs you ; for 32m10s"
        );
        let filled: Vec<(&str, Tone, Option<Fill>)> = lines[0]
            .spans()
            .iter()
            .filter(|span| span.fill.is_some())
            .map(|span| (span.text.as_str(), span.tone, span.fill))
            .collect();
        assert_eq!(filled, [("needs you", Tone::NeedsYou, Some(Fill::Pill))]);
        assert_eq!(lines[0].fill(), None);
    }
}

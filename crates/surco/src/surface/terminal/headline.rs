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
    /// types next, but an edge something else in sight says, which gives way
    /// whole first ([`Headline::edge_shown`]). The word is filled as a pill in
    /// the mark's tone, padded by a space of each side it takes from what
    /// stands beside it: a layer
    /// drawing 24-bit color paints it as a badge, and every other layer draws
    /// the same text, the word in the tone's color where that layer has one.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let glyphs = look.glyphs;
        let tone = Tone::of(self.mark);
        let sep = format!(" {} ", glyphs.sep());
        let mark = format!("{} ", glyphs.mark(self.mark));
        let edge = self.edge_shown(look.width.cells(), &mark, &sep);
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
        let facts = fitting(&self.facts, &sep, &mut used, room);
        // The space after the pill is the next fact's, or the gap's before
        // the edge; with neither, the line ends at the word.
        let after = match facts.is_empty() && edge.is_empty() {
            true => "",
            false => " ",
        };
        let mut line = Line::new()
            .push(tone, mark.trim_end())
            .plain(" ")
            .push(Tone::Strong, subject)
            .push(Tone::Muted, sep.trim_end())
            .push_filled(tone, Fill::Pill, format!(" {}{after}", self.said));
        for (at, fact) in facts.iter().enumerate() {
            let lead = match at {
                0 => sep.trim_start(),
                _ => sep.as_str(),
            };
            line = line.push(Tone::Muted, format!("{lead}{fact}"));
        }
        if !edge.is_empty() {
            let gap = look
                .width
                .cells()
                .saturating_sub(used + cell_width(edge))
                .max(2);
            let gap = gap - usize::from(facts.is_empty());
            line = line.plain(" ".repeat(gap)).plain(edge);
        }
        vec![line]
    }
}

impl Headline {
    /// The edge a line `cells` wide shows, `mark` and `sep` as it draws them: none where
    /// something else in sight says it and the whole headline, every fact said, would not
    /// stand beside it with the two cells between them; the edge, or nothing, otherwise.
    fn edge_shown(&self, cells: usize, mark: &str, sep: &str) -> &str {
        let edge = self.edge.as_deref().unwrap_or_default();
        let facts: usize = self
            .facts
            .iter()
            .map(|fact| cell_width(sep) + cell_width(fact))
            .sum();
        let whole = cell_width(mark)
            + cell_width(&self.subject)
            + cell_width(sep)
            + cell_width(&self.said)
            + facts;
        match self.edge_said_elsewhere && whole + 2 + cell_width(edge) > cells {
            true => "",
            false => edge,
        }
    }
}

/// The facts that fit `room` after the `used` cells, in order, each after
/// `sep`; `used` grows by each one taken.
fn fitting<'a>(facts: &'a [String], sep: &str, used: &mut usize, room: usize) -> Vec<&'a str> {
    let mut taken = Vec::new();
    for fact in facts {
        let takes = cell_width(sep) + cell_width(fact);
        if *used + takes > room {
            break;
        }
        *used += takes;
        taken.push(fact.as_str());
    }
    taken
}

#[cfg(test)]
mod tests {
    use crate::components::Headline;
    use crate::glyphs::Mark;
    use crate::line::{Fill, Tone};
    use crate::look::Look;

    /// The standing word is the one span of a headline that is filled, as a
    /// pill in its mark's tone padded by a space of each side. The words
    /// around it read the same, unfilled.
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
        assert_eq!(filled, [(" needs you ", Tone::NeedsYou, Some(Fill::Pill))]);
        assert_eq!(lines[0].fill(), None);
    }

    /// With no fact and no edge after it, the pill takes only the space
    /// before it, so the line ends at its word as it did.
    #[test]
    fn a_pill_at_the_end_of_its_line_takes_no_space_after_it() {
        let headline = Headline::new(Mark::Done, "fragua (standard)", "finished");
        let lines = headline.lines(&Look::plain());
        assert_eq!(lines[0].text(), "+ fragua (standard) ; finished");
        let filled: Vec<&str> = lines[0]
            .spans()
            .iter()
            .filter(|span| span.fill.is_some())
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(filled, [" finished"]);
    }

    /// A unicode terminal of 60 columns, inside its margins.
    fn sixty() -> Look {
        Look {
            glyphs: crate::glyphs::Glyphs::Unicode,
            width: crate::look::Width::exact(58),
            ..Look::plain()
        }
    }

    /// Where something else in sight says the edge, as a run's screen's trail says its
    /// handle, a line too narrow for the whole headline lets the edge go before a fact or
    /// the subject, as the targets `board-tasks` and `orphans` draw their third row at 60
    /// columns; one too narrow without the edge lets its facts go from the last.
    #[test]
    fn an_edge_said_elsewhere_gives_way_first() {
        let drawn = |headline: Headline| headline.lines(&sixty())[0].text();
        let working = Headline::new(Mark::Running, "independent-tasks (default)", "running")
            .fact("for 2m27s")
            .edge_said_elsewhere("TW0TSK");
        assert_eq!(
            drawn(working),
            "● independent-tasks (default) · running · for 2m27s"
        );
        let parked = Headline::new(Mark::NeedsYou, "uncertain-on-crash (default)", "needs you")
            .fact("for 0s")
            .edge_said_elsewhere("0RPHAN");
        assert_eq!(
            drawn(parked),
            "◆ uncertain-on-crash (default) · needs you · for 0s"
        );
        let fits = Headline::new(Mark::Running, "fragua (standard)", "running")
            .fact("for 7m34s")
            .edge_said_elsewhere("Q78GRR");
        assert_eq!(
            drawn(fits),
            "● fragua (standard) · running · for 7m34s           Q78GRR"
        );
        let longer = Headline::new(Mark::Running, "independent-tasks (default)", "running")
            .fact("for 2m27s")
            .fact("and a fact past the row")
            .edge_said_elsewhere("TW0TSK");
        assert_eq!(
            drawn(longer),
            "● independent-tasks (default) · running · for 2m27s"
        );
    }

    /// A page keeps its edge, which a reader types next, and lets its facts go instead.
    #[test]
    fn a_pages_edge_never_gives_way() {
        let page = Headline::new(Mark::Running, "independent-tasks (default)", "running")
            .fact("for 2m27s")
            .edge("TW0TSK");
        let drawn = page.lines(&sixty())[0].text();
        assert!(drawn.ends_with("  TW0TSK"), "{drawn}");
        assert!(!drawn.contains("2m27s"), "{drawn}");
    }
}

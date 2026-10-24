//! A problem pointed at in its line, and a command's refusal.

use crate::components::{Place, Problem, Refusal, Severity};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap_unbroken, INDENT};

/// What a refusal opens with, on every surface.
pub(in crate::surface) const REFUSED: &str = "error:";

impl Severity {
    fn tone(self) -> Tone {
        match self {
            Severity::Error => Tone::Failed,
            Severity::Warning => Tone::Caution,
        }
    }
}

impl Problem {
    /// Its word in its tone and what it says, wrapped under itself; then
    /// the place: where it is, the line as written, and the part at fault
    /// marked under it.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let tone = self.severity.tone();
        let word = self.severity.word();
        let under = format!("{INDENT}{}", " ".repeat(cell_width(word) + 2));
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines: Vec<Line> = wrap_unbroken(&self.said, room)
            .into_iter()
            .enumerate()
            .map(|(at, part)| match at {
                0 => Line::new()
                    .plain(INDENT)
                    .push(tone, word)
                    .plain("  ")
                    .plain(part),
                _ => Line::new().plain(under.as_str()).plain(part),
            })
            .collect();
        if let Some(place) = &self.at {
            lines.extend(place.lines(tone, look));
        }
        lines
    }
}

impl Place {
    /// Where the problem is, then its line between a gutter and the line's
    /// number, and the part at fault marked under it in `tone`.
    fn lines(&self, tone: Tone, look: &Look) -> Vec<Line> {
        let number = self.line.to_string();
        let pad = " ".repeat(cell_width(&number));
        let gutter = format!(" {} ", look.glyphs.gutter());
        let mut lines = vec![Line::new()
            .plain(INDENT)
            .plain(pad.as_str())
            .push(Tone::Muted, look.glyphs.points_at())
            .plain(" ")
            .plain(self.named())];
        let Some((source, before, len)) = self.fault() else {
            return lines;
        };
        let marks = look.glyphs.fault().to_string().repeat(len);
        lines.extend([
            Line::new()
                .plain(INDENT)
                .plain(pad.as_str())
                .push(Tone::Muted, gutter.trim_end()),
            Line::new()
                .plain(INDENT)
                .push(Tone::Muted, number)
                .push(Tone::Muted, gutter.as_str())
                .plain(source),
            Line::new()
                .plain(INDENT)
                .plain(pad.as_str())
                .push(Tone::Muted, gutter.as_str())
                .plain(" ".repeat(cell_width(&before)))
                .push(tone, marks),
        ]);
        lines
    }
}

impl Refusal {
    /// `error:` in the failed tone and the sentence, wrapped under itself;
    /// a line the sentence breaks on its own stays as its author wrote it.
    /// Where it looked comes under it, muted.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let mut written = self.said.lines();
        let first = written.next().unwrap_or_default();
        let room = look.width.cells().saturating_sub(cell_width(REFUSED) + 1);
        let mut lines: Vec<Line> = wrap_unbroken(first, room)
            .into_iter()
            .enumerate()
            .map(|(at, part)| match at {
                0 => Line::new()
                    .push(Tone::Failed, REFUSED)
                    .plain(" ")
                    .plain(part),
                _ => Line::new().plain(INDENT).plain(part),
            })
            .collect();
        lines.extend(written.map(|line| Line::new().plain(line)));
        if let Some(detail) = &self.detail {
            lines.push(Line::new().plain(INDENT).push(Tone::Muted, detail.as_str()));
        }
        lines
    }
}

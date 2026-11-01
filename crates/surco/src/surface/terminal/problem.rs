//! A problem pointed at in its line, and a command's refusal.

use crate::components::{Place, Problem, Refusal, Severity};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, packed, wrap_unbroken, INDENT};

/// What a refusal opens with, on every surface.
pub(in crate::surface) const REFUSED: &str = "error:";

/// What the line of what helps fix a problem opens with, on every surface.
pub(in crate::surface) const HELP: &str = "help:";

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
    /// marked under it; then what helps fix it, on a `help:` line under
    /// them.
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
        lines.extend(self.helped(look));
        lines
    }

    /// What helps fix it, on one line in the gutter's column under its
    /// place, or as far in when it has none: each a separator apart, a line
    /// broken only between two, wrapped under itself.
    fn helped(&self, look: &Look) -> Vec<Line> {
        if self.help.is_empty() {
            return Vec::new();
        }
        let lead = match &self.at {
            Some(place) => format!("{INDENT}{} ", place.pad()),
            None => format!("{INDENT}{INDENT}"),
        };
        let under = format!("{lead}{}", " ".repeat(cell_width(HELP) + 1));
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let items: Vec<&str> = self.help.iter().map(String::as_str).collect();
        packed(&items, look.glyphs.sep(), room)
            .into_iter()
            .enumerate()
            .map(|(at, part)| match at {
                0 => Line::new()
                    .plain(lead.as_str())
                    .push(Tone::Strong, HELP)
                    .plain(" ")
                    .plain(part),
                _ => Line::new().plain(under.as_str()).plain(part),
            })
            .collect()
    }
}

impl Place {
    /// As many spaces as the line's number is wide: what sets the gutter
    /// in the same column on every row of the place, and what helps fix
    /// the problem under it.
    pub(in crate::surface) fn pad(&self) -> String {
        " ".repeat(cell_width(&self.line.to_string()))
    }

    /// Where the problem is, then its line between a gutter and the line's
    /// number, and the part at fault marked under it in `tone`.
    fn lines(&self, tone: Tone, look: &Look) -> Vec<Line> {
        let number = self.line.to_string();
        let pad = self.pad();
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
        let indent = " ".repeat(cell_width(&before));
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
                .plain(indent)
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

#[cfg(test)]
mod tests {
    use crate::components::{Place, Problem};
    use crate::look::Look;

    #[test]
    fn what_helps_fix_a_problem_starts_in_its_gutter_column_whatever_the_line_number() {
        for line in [9, 12, 100] {
            let problem = Problem::error(
                "unknown key `promt`",
                Some(Place {
                    path: "wf.yaml".to_string(),
                    line,
                    col: 5,
                    source: Some("    promt: p".to_string()),
                    len: 5,
                }),
            )
            .help(["did you mean `prompt`?".to_string()]);
            let look = Look::plain();
            let drawn: Vec<String> = problem
                .lines(&look)
                .iter()
                .map(|row| look.paint.line(row))
                .collect();
            let gutter = drawn
                .iter()
                .find(|row| row.trim() == "|")
                .and_then(|row| row.find('|'))
                .expect("a gutter row");
            let help = drawn
                .iter()
                .find_map(|row| row.find("help:"))
                .expect("a help line");
            assert_eq!(help, gutter, "line {line}: {drawn:#?}");
        }
    }
}

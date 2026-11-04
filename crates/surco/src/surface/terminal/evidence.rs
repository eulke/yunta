//! The end of what a command printed, and where the rest is.

use crate::components::{Evidence, Whole};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, truncate, INDENT};

impl Evidence {
    /// The last `QUOTED` lines, each hanging from the gutter and cut to
    /// the line rather than wrapped — what a command printed keeps its
    /// shape — then how much came before them and where it is.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let gutter = format!("{INDENT}{} ", look.glyphs.gutter());
        let room = look.width.cells().saturating_sub(cell_width(&gutter));
        let skipped = self.above();
        let mut lines: Vec<Line> = self
            .quoted()
            .iter()
            .map(|said| {
                Line::new()
                    .push(Tone::Muted, gutter.as_str())
                    .plain(truncate(said, room, look.glyphs).trim_end())
            })
            .collect();
        let mut rest: Vec<Line> = Vec::new();
        if skipped > 0 {
            rest.push(Line::new().push(
                Tone::Muted,
                format!("{} above", crate::text::counted(skipped, "line")),
            ));
        }
        if let Some(whole) = &self.whole {
            let said = Line::new().push(Tone::Muted, "whole output: ");
            rest.push(match whole {
                Whole::File { shown, path } => said.path(shown.as_str(), path),
                Whole::Command(command) => said.push(Tone::Strong, command.as_str()),
            });
        }
        // One line when it fits, and each fact on its own when it does
        // not: a path cut to fit is a path nobody can open.
        let sep = format!(" {} ", look.glyphs.sep());
        let joined = rest.iter().map(Line::text).collect::<Vec<_>>().join(&sep);
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        match cell_width(&joined) <= room {
            true if !rest.is_empty() => {
                let mut line = Line::new().plain(INDENT);
                for (at, part) in rest.into_iter().enumerate() {
                    if at > 0 {
                        line = line.push(Tone::Muted, sep.as_str());
                    }
                    line = line.then(part);
                }
                lines.push(line);
            }
            _ => lines.extend(rest.into_iter().map(|part| part.under(INDENT))),
        }
        lines
    }
}

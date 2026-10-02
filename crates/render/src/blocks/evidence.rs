//! What a command printed, quoted: the end of it, and where the rest is.

use std::path::PathBuf;

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::{cell_width, truncate, Look, INDENT};

/// The lines evidence quotes at most: the end of what a command printed,
/// which is where a compiler and a test runner say what went wrong. More
/// buries the verdict under the output it came from; the rest is one
/// command or one path away.
const QUOTED: usize = 6;

/// Where the whole of what a command printed is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Whole {
    /// The file it is kept in: shown as a reader reads a path, and a
    /// link to where it is on a terminal that opens one.
    File { shown: String, path: PathBuf },
    /// The command that shows it.
    Command(String),
}

impl Whole {
    /// The file at `path`, shown from `cwd` and with `~` for `home`.
    pub fn file(path: PathBuf, cwd: &std::path::Path, home: Option<&std::path::Path>) -> Self {
        Whole::File {
            shown: crate::paths::shown(&path, cwd, home),
            path,
        }
    }
}

/// The end of what a command printed, and where the whole of it is.
pub struct Evidence {
    pub tail: Vec<String>,
    pub whole: Option<Whole>,
}

impl Evidence {
    /// The lines quoted: the last [`QUOTED`] of the tail, where a
    /// compiler or a test runner says what failed.
    pub fn quoted(&self) -> &[String] {
        let skipped = self.tail.len().saturating_sub(QUOTED);
        self.tail.get(skipped..).unwrap_or_default()
    }

    /// How many lines of the tail are left above what is quoted.
    pub fn above(&self) -> usize {
        self.tail.len() - self.quoted().len()
    }
}

impl Drawn for Evidence {
    /// The last [`QUOTED`] lines, each hanging from the gutter and cut to
    /// the line rather than wrapped — what a command printed keeps its
    /// shape — then how much came before them and where it is.
    fn lines(&self, look: &Look) -> Vec<Line> {
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
                format!("{} above", yunta_core::text::counted(skipped, "line")),
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

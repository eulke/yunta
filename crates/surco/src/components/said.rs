//! What a message says when a block of its own says it whole: its lines as
//! its author broke them, or a heading and each path it names.

use std::path::PathBuf;

use crate::components::Drawn;
use crate::line::Line;
use crate::look::Look;
use crate::width::{indent, INDENT};

/// Lines said whole, as their author broke them: a message that runs over
/// more than one line, which a surface neither wraps nor joins.
pub struct Said(pub Vec<String>);

impl Drawn for Said {
    fn lines(&self, _look: &Look) -> Vec<Line> {
        self.0
            .iter()
            .map(|said| Line::new().plain(INDENT).plain(said.as_str()))
            .collect()
    }
}

/// A path a message names, and the reason it is named for, when the
/// message gives one.
pub struct PathNote {
    pub path: PathBuf,
    pub reason: Option<String>,
}

/// A heading, and each path it names.
pub struct Paths {
    pub heading: String,
    pub paths: Vec<PathNote>,
}

impl Drawn for Paths {
    fn lines(&self, _look: &Look) -> Vec<Line> {
        std::iter::once(Line::new().plain(INDENT).plain(self.heading.as_str()))
            .chain(self.paths.iter().map(|note| {
                let line = Line::new()
                    .plain(indent(2))
                    .plain(note.path.display().to_string());
                match &note.reason {
                    Some(reason) => line.plain(" — ").plain(reason.as_str()),
                    None => line,
                }
            }))
            .collect()
    }
}

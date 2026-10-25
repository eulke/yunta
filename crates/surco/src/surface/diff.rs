//! Code read against the code it changes, as every surface draws it: the
//! words a change puts in and takes out between git's word-diff marks, each
//! in a tone of its own, and the words it keeps as written.

use crate::components::{Against, Code, Part};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};

/// Each line of `code` as a surface draws it: marked where the code
/// modifies what stands where it goes, as written otherwise.
pub(in crate::surface) fn lines(code: &Code, glyphs: Glyphs) -> Vec<Line> {
    let marks = match &code.against {
        Some(Against::Modified(Some(marks))) => marks.lines.as_slice(),
        _ => &[],
    };
    code.lines
        .iter()
        .enumerate()
        .map(|(at, line)| match marks.get(at) {
            Some(parts) => marked(parts, glyphs),
            None => Line::new().plain(line.as_str()),
        })
        .collect()
}

/// `parts`, one line of modified code, as the line a surface draws: what it
/// keeps as written, and each run it puts in or takes out between its marks,
/// in its tone.
fn marked(parts: &[Part], glyphs: Glyphs) -> Line {
    parts.iter().fold(Line::new(), |line, part| match part {
        Part::Kept(text) => line.plain(text.as_str()),
        Part::Added(text) => {
            let [open, close] = glyphs.added();
            line.push(Tone::Added, format!("{open}{text}{close}"))
        }
        Part::Removed(text) => {
            let [open, close] = glyphs.removed();
            line.push(Tone::Removed, format!("{open}{text}{close}"))
        }
    })
}

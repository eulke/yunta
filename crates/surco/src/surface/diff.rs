//! Code read against the code it changes, as every surface draws it: the
//! words a change puts in and takes out between git's word-diff marks, each
//! in a tone of its own, and the words it keeps as written. Every other word
//! takes the tone of what it is in the code's language, so a 24-bit stream
//! colors it as code.

use std::ops::Range;

use crate::components::{Against, Code, Part};
use crate::glyphs::Glyphs;
use crate::line::{Line, Syntax, Tone};

/// Each line of `code` as a surface draws it, each word in the tone of
/// what it is in the code's language: marked where the code modifies
/// what stands where it goes, as written otherwise.
pub(in crate::surface) fn lines(code: &Code, glyphs: Glyphs) -> Vec<Line> {
    let marks = match &code.against {
        Some(Against::Modified(Some(marks))) => marks.lines.as_slice(),
        _ => &[],
    };
    let text = code.lines.join("\n");
    let colored = crate::code::highlighted(&text, crate::code::Language::of(&code.at));
    let mut start = 0;
    code.lines
        .iter()
        .enumerate()
        .map(|(at, line)| {
            let range = start..start + line.len();
            start = range.end + 1;
            match marks.get(at) {
                Some(parts) => marked(parts, glyphs, &text, range.start, &colored),
                None => crate::code::painted(&text, range, &colored),
            }
        })
        .collect()
}

/// `parts`, one line of modified code that starts at byte `start` of
/// `text`, as the line a surface draws: what it keeps in the tone of what
/// each word is, and each run it puts in or takes out between its marks,
/// in its tone.
fn marked(
    parts: &[Part],
    glyphs: Glyphs,
    text: &str,
    start: usize,
    colored: &[(Range<usize>, Syntax)],
) -> Line {
    let mut line = Line::new();
    let mut at = start;
    for part in parts {
        line = match part {
            Part::Kept(kept) => {
                let stretch = at..at + kept.len();
                at = stretch.end;
                // What a line keeps is a slice of the code it stands in;
                // were it not, it is drawn as written.
                match text.get(stretch.clone()) {
                    Some(said) if said == kept => {
                        line.then(crate::code::painted(text, stretch, colored))
                    }
                    _ => line.plain(kept.as_str()),
                }
            }
            Part::Added(added) => {
                at += added.len();
                let [open, close] = glyphs.added();
                line.push(Tone::Added, format!("{open}{added}{close}"))
            }
            Part::Removed(removed) => {
                let [open, close] = glyphs.removed();
                line.push(Tone::Removed, format!("{open}{removed}{close}"))
            }
        };
    }
    line
}

#[cfg(test)]
mod tests;

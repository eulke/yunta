//! How Markdown writes its code: a fence, three or more backticks or tildes
//! that open a line, and a code span, a run of backticks that a run of as
//! many closes. A fence is read as [`blocks`] reads one, its marks three spaces
//! into the block that holds them at most.

mod blocks;

pub(crate) use blocks::{fence_left_open, MIN_FENCE};

use std::ops::Range;

use super::super::{bytes_from, run, Holes, Language, Literal};
use super::escaped;

/// The Markdown literal that starts at `at`, if one does: a fence, one of
/// `fences`; or a code span, a run of backticks that a run of
/// exactly as many closes before a blank line or a fence ends the paragraph.
/// A run none closes is text, as a backtick a `\` escapes is.
pub(in super::super) fn markdown(source: &[u8], at: usize, fences: &Fences) -> Option<Literal> {
    let mark = *source
        .get(at)
        .filter(|&&byte| byte == b'`' || byte == b'~')?;
    let before = at.checked_sub(1).and_then(|before| source.get(before));
    if before == Some(&mark) || escaped(source, at) {
        return None;
    }
    let length = run(source, at, |byte| byte == mark);
    if let Some(fence) = fences.at(at) {
        return Some(Literal::ending(at + length, fence.end(), Holes::None));
    }
    if mark == b'~' {
        return None;
    }
    let end = span_end(source, at + length, length, fences)?;
    Some(Literal::ending(at + length, end, Holes::None))
}

/// A Markdown fence: its opening line, its info string, its code, and the
/// line that closes it, where one does.
pub(crate) struct Fence {
    /// The opening line, from its marks to its end.
    pub(crate) opening: Range<usize>,
    /// Its info string: what follows the opening marks, its blanks trimmed.
    pub(crate) info: Range<usize>,
    /// Its code: the lines between the opening line and the closing one, or
    /// through the source's end.
    pub(crate) code: Range<usize>,
    /// The closing line, from its marks to its end, where one closes it.
    pub(crate) closing: Option<Range<usize>>,
    /// How many spaces stand before its marks: what each line of its code gives up.
    pub(crate) indent: usize,
    /// Where what follows it starts: the line after the one that closes it, the line that ended
    /// the list item that holds it, or the source's end.
    pub(crate) next: usize,
}

impl Fence {
    /// Where the fence ends: at its closing line's end, or where its code does.
    pub(crate) fn end(&self) -> usize {
        self.closing
            .as_ref()
            .map_or(self.code.end, |closing| closing.end)
    }
}

/// The fences of one text, read once: what every reader of its blocks asks of it, however many
/// lines it asks about.
#[derive(Default)]
pub(crate) struct Fences(Vec<Fence>);

impl Fences {
    /// The fences `text` holds, in the order it has them.
    pub(crate) fn of(text: &str) -> Fences {
        Fences(blocks::read(text).fences)
    }

    /// The fences of `text` if it is Markdown, the one language that writes any.
    pub(crate) fn in_markdown(text: &str, language: Option<Language>) -> Fences {
        match language {
            Some(Language::Markdown) => Fences::of(text),
            _ => Fences::default(),
        }
    }

    /// The fence whose marks start at `at`, if one does.
    pub(crate) fn at(&self, at: usize) -> Option<&Fence> {
        let found = self
            .0
            .binary_search_by_key(&at, |fence| fence.opening.start)
            .ok()?;
        self.0.get(found)
    }
}

/// Where the code span whose text starts at `from` ends, `length` backticks
/// having opened it: just past the next run of exactly as many, if one
/// stands before a blank line, or a line a fence opens, ends the paragraph.
fn span_end(source: &[u8], from: usize, length: usize, fences: &Fences) -> Option<usize> {
    let mut at = from;
    while let Some(&byte) = source.get(at) {
        match byte {
            b'`' => {
                let ticks = run(source, at, |byte| byte == b'`');
                if ticks == length {
                    return Some(at + ticks);
                }
                at += ticks;
            }
            b'\n' if blank_line(source, at + 1) || opens_fence(source, at + 1, fences) => {
                return None
            }
            _ => at += 1,
        }
    }
    None
}

/// Whether the line that starts at `at` holds nothing but spaces and tabs.
fn blank_line(source: &[u8], at: usize) -> bool {
    bytes_from(source, at)
        .iter()
        .take_while(|&&byte| byte != b'\n')
        .all(u8::is_ascii_whitespace)
}

/// Whether a fence opens on the line that starts at `line`: a fence ends the
/// paragraph before it, and the code spans that paragraph holds, even a line
/// of as many backticks as a span opened with.
fn opens_fence(source: &[u8], line: usize, fences: &Fences) -> bool {
    let marks = line + run(source, line, |byte| byte == b' ');
    fences.at(marks).is_some()
}

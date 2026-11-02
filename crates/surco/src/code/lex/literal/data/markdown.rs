//! How Markdown writes its code: a fence, three or more backticks or tildes
//! that open a line, and a code span, a run of backticks that a run of as
//! many closes. A fence is read as CommonMark reads one at the top of a
//! document, its marks three spaces into their line at most; a fence in a
//! list item is read by that indentation alone.

use std::ops::Range;

use super::super::{bytes_from, line_end, run, Holes, Literal};
use super::{escaped, line_start};

/// The most spaces a fence's marks may stand into their line: four make the
/// line indented code, as CommonMark reads it.
const MAX_INDENT: usize = 3;

/// The Markdown literal that starts at `at`, if one does: a fence, as
/// [`Fence::at`] reads one; or a code span, a run of backticks that a run of
/// exactly as many closes before a blank line or a fence ends the paragraph.
/// A run none closes is text, as a backtick a `\` escapes is.
pub(in super::super) fn markdown(source: &[u8], at: usize) -> Option<Literal> {
    let mark = *source
        .get(at)
        .filter(|&&byte| byte == b'`' || byte == b'~')?;
    let before = at.checked_sub(1).and_then(|before| source.get(before));
    if before == Some(&mark) || escaped(source, at) {
        return None;
    }
    let length = run(source, at, |byte| byte == mark);
    if let Some(fence) = Fence::at(source, at) {
        return Some(Literal::ending(at + length, fence.end(source), Holes::None));
    }
    if mark == b'~' {
        return None;
    }
    let end = span_end(source, at + length, length)?;
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
}

impl Fence {
    /// The fence whose marks start at `at`, if one does: three or more
    /// backticks or tildes, three spaces into their line at most, a backtick
    /// fence's info string holding no backtick. It runs on through the line
    /// that closes it, as many of its marks or more, three spaces in at most
    /// and with nothing after them but spaces, or to the source's end.
    pub(crate) fn at(source: &[u8], at: usize) -> Option<Fence> {
        let (mark, length) = opener(source, at)?;
        let opened = line_end(source, at);
        let after = source.get(at + length..opened).unwrap_or_default();
        let lead = after.len() - after.trim_ascii_start().len();
        let info = at + length + lead..at + length + lead + after.trim_ascii().len();
        let start = (opened + 1).min(source.len());
        let closing = closing_line(source, start, mark, length);
        // The code ends at the line break before the closing line.
        let end = closing.as_ref().map_or(source.len(), |closing| {
            line_start(source, closing.start)
                .saturating_sub(1)
                .max(start)
        });
        Some(Fence {
            opening: at..opened,
            info,
            code: start..end,
            closing,
        })
    }

    /// Where the fence ends: at its closing line's end, or at the source's.
    pub(crate) fn end(&self, source: &[u8]) -> usize {
        self.closing
            .as_ref()
            .map_or(source.len(), |closing| closing.end)
    }
}

/// The mark of the fence that opens at `at`, and how many of it there are,
/// if one does: three or more backticks or tildes with at most three spaces
/// before them on their line, and after backticks an info string with no
/// backtick in it.
fn opener(source: &[u8], at: usize) -> Option<(u8, usize)> {
    let mark = *source
        .get(at)
        .filter(|&&byte| byte == b'`' || byte == b'~')?;
    let length = run(source, at, |byte| byte == mark);
    let lead = source.get(line_start(source, at)..at).unwrap_or_default();
    let info = source
        .get(at + length..line_end(source, at))
        .unwrap_or_default();
    let opens = length >= 3
        && lead.len() <= MAX_INDENT
        && lead.iter().all(|&byte| byte == b' ')
        && (mark == b'~' || !info.contains(&b'`'));
    opens.then_some((mark, length))
}

/// The line that closes a fence of `length` marks `mark` whose code starts
/// at `start`, from its marks to its end: the first line from `start` that
/// holds as many of the marks or more, three spaces in at most, and nothing
/// else but spaces.
fn closing_line(source: &[u8], start: usize, mark: u8, length: usize) -> Option<Range<usize>> {
    let mut line = start;
    while line < source.len() {
        let close = line_end(source, line);
        let said = source.get(line..close).unwrap_or_default();
        let lead = said.iter().take_while(|&&byte| byte == b' ').count();
        let marks = said.get(lead..).unwrap_or_default().trim_ascii_end();
        let count = marks.iter().take_while(|&&byte| byte == mark).count();
        if lead <= MAX_INDENT && count >= length && count == marks.len() {
            return Some(line + lead..close);
        }
        line = close + 1;
    }
    None
}

/// Where the code span whose text starts at `from` ends, `length` backticks
/// having opened it: just past the next run of exactly as many, if one
/// stands before a blank line, or a line a fence opens, ends the paragraph.
fn span_end(source: &[u8], from: usize, length: usize) -> Option<usize> {
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
            b'\n' if blank_line(source, at + 1) || opens_fence(source, at + 1) => return None,
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
fn opens_fence(source: &[u8], line: usize) -> bool {
    let marks = line + run(source, line, |byte| byte == b' ');
    opener(source, marks).is_some()
}

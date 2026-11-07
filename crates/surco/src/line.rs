//! What a line says, apart from how it is painted.
//!
//! A line is built from spans that each carry a tone — what the text is
//! to a reader — and painted once, by the paint its stream gets
//! ([`crate::paint`]). Color only repeats what the words already say, so
//! every paint writes the same text. A span or a line may also carry a
//! fill, a background that only a layer drawing 24-bit color paints.

use std::ops::Range;

use crate::glyphs::{Glyphs, Mark};
use crate::width::{cell_width, truncate};

/// What a span of text is to a reader, which decides its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The body of a line: the terminal's own color.
    Plain,
    /// What a line is about.
    Strong,
    /// A command to copy, drawn as strongly as what a line is about.
    Command,
    /// What a reader skims past: a label, a duration, a path.
    Muted,
    /// A stroke that says nothing by itself, which the words beside it say:
    /// the stretch of a furrow still ahead, the ground a page stands on.
    /// Fainter than what a reader skims past.
    Faint,
    Done,
    Failed,
    Running,
    /// Something waits on a person. The one tone that means a person.
    NeedsYou,
    /// Worth a reader's attention and nothing anybody has to answer.
    Caution,
    /// Words a change puts into a line of code, with their marks.
    Added,
    /// Words a change takes out of a line of code, with their marks.
    Removed,
    /// A word of code, by what it is.
    Code(Syntax),
}

/// What a word of code is to a reader of code, which decides its color
/// where code is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// A word the language reserves, `fn`, `pub`, `let`, or one that names a
    /// part of a document's structure: a key, a flag, a heading below the
    /// first level.
    Keyword,
    /// A type, `PathBuf`, `bool`, a name that opens with a capital, or what
    /// names a whole part of a document: a table's header, a heading of the
    /// first level.
    Type,
    /// A function: the name a declaration gives or a call calls.
    Function,
    /// The text of a string or a character, its quotes included.
    Text,
    /// A number.
    Number,
    /// A comment, which recedes.
    Comment,
    /// An attribute or a decorator: `#[test]`, `@cached`.
    Attribute,
    /// A macro: `format!`.
    Macro,
}

impl Tone {
    /// The tone a mark is painted in.
    pub fn of(mark: Mark) -> Self {
        match mark {
            Mark::Done => Tone::Done,
            Mark::Failed => Tone::Failed,
            Mark::Running => Tone::Running,
            Mark::NeedsYou => Tone::NeedsYou,
            Mark::Caution | Mark::Reroute => Tone::Caution,
            Mark::Stopped | Mark::Skipped | Mark::Absent => Tone::Muted,
        }
    }
}

/// A background behind a span or a line, which only a layer drawing 24-bit
/// color paints. The words read the same with it or without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// The decision that waits, set apart from the page.
    Tint,
    /// The row a pointer stands on, set apart from the rest of a decision.
    Selection,
    /// Code shown as code.
    Code,
    /// A word a change adds.
    Added,
    /// A word a change removes.
    Removed,
    /// A word set as a pill: its standing, in its own color.
    Pill,
}

/// A run of text in one tone, and — for a path a reader opens — what it
/// links to where a terminal opens links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub tone: Tone,
    pub link: Option<String>,
    /// The background behind this span; a fill of its own wins over the line's.
    pub fill: Option<Fill>,
}

/// One line, as the spans it is built from, and the fill behind all of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    /// The spans the line is made of, in order.
    spans: Vec<Span>,
    /// The fill behind the whole line; a span's own fill wins over it.
    fill: Option<Fill>,
}

impl Line {
    pub fn new() -> Self {
        Self::default()
    }

    /// The line with `text` appended in `tone`.
    pub fn push(self, tone: Tone, text: impl Into<String>) -> Self {
        self.push_with_fill(tone, None, text)
    }

    /// The line with `text` appended in `tone`, on `fill`.
    pub fn push_filled(self, tone: Tone, fill: Fill, text: impl Into<String>) -> Self {
        self.push_with_fill(tone, Some(fill), text)
    }

    /// The line with `text` in `tone` and `fill`, if any. Empty text adds nothing.
    fn push_with_fill(mut self, tone: Tone, fill: Option<Fill>, text: impl Into<String>) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.spans.push(Span {
                text,
                tone,
                link: None,
                fill,
            });
        }
        self
    }

    /// The line with `shown` appended as a path, linking to the file at
    /// `target` on a terminal that opens links.
    pub fn path(mut self, shown: impl Into<String>, target: &std::path::Path) -> Self {
        let text = shown.into();
        if !text.is_empty() {
            self.spans.push(Span {
                text,
                tone: Tone::Plain,
                link: Some(file_url(target)),
                fill: None,
            });
        }
        self
    }

    /// The line with `text` appended in the terminal's own color.
    pub fn plain(self, text: impl Into<String>) -> Self {
        self.push(Tone::Plain, text)
    }

    /// The line with `fill` behind all of it, under any span with a fill of its own.
    pub fn filled(mut self, fill: Fill) -> Self {
        self.fill = Some(fill);
        self
    }

    /// The fill behind the whole line, if it has one.
    pub fn fill(&self) -> Option<Fill> {
        self.fill
    }

    /// The line with every span in `tone` that has no fill of its own on
    /// `fill`: how the words a change marks in a line of code stand out from
    /// the code around them.
    pub fn fill_tone(mut self, tone: Tone, fill: Fill) -> Self {
        for span in &mut self.spans {
            if span.tone == tone && span.fill.is_none() {
                span.fill = Some(fill);
            }
        }
        self
    }

    /// The line moved right by `margin`, its spans, tones and fills kept.
    pub fn under(mut self, margin: &str) -> Self {
        if !margin.is_empty() {
            self.spans.insert(
                0,
                Span {
                    text: margin.to_string(),
                    tone: Tone::Plain,
                    link: None,
                    fill: None,
                },
            );
        }
        self
    }

    /// The line inside `cells`, cut where it passes them and marked with
    /// the ellipsis `glyphs` draws, each span keeping its tone and fill —
    /// so a row is cut on what a reader sees and painted after. The
    /// ellipsis takes the tone and fill of the span the cut falls in, or
    /// of the span it ends when it falls between two.
    pub fn cut(&self, cells: usize, glyphs: Glyphs) -> Line {
        let plain: String = self.spans.iter().map(|span| span.text.as_str()).collect();
        if cell_width(&plain) <= cells {
            return self.clone();
        }
        let ellipsis = glyphs.ellipsis();
        let cut = truncate(&plain, cells, glyphs);
        let kept = cut.trim_end().strip_suffix(ellipsis).unwrap_or_default();
        let mut line = self.empty_like();
        let mut left = kept.len();
        for span in &self.spans {
            // The cut lands between characters of the whole line, so it
            // lands between characters of the span it falls in.
            let taken = span.text.len().min(left);
            left -= taken;
            if taken > 0 {
                line.spans.push(Span {
                    text: span.text[..taken].to_string(),
                    ..span.clone()
                });
            }
            if left == 0 {
                return line.push_with_fill(span.tone, span.fill, ellipsis);
            }
        }
        line
    }

    /// The part of the line in `range`, bytes of its text, each span it
    /// cuts keeping its tone, fill and link: a piece of a line broken to
    /// fit is painted as the line is.
    pub fn slice(&self, range: Range<usize>) -> Line {
        let mut sliced = self.empty_like();
        let mut start = 0;
        for span in &self.spans {
            let end = start + span.text.len();
            let cut = range.start.max(start) - start..range.end.min(end).max(start) - start;
            if let Some(text) = span.text.get(cut).filter(|text| !text.is_empty()) {
                sliced.spans.push(Span {
                    text: text.to_string(),
                    ..span.clone()
                });
            }
            start = end;
        }
        sliced
    }

    /// The line as lines inside `cells`, broken between words, each part
    /// of a word in the tone, fill and link of the span it came from. A
    /// word wider than the line is a line of its own, whole: a path or a
    /// command cut in two reads as two. What the lines start with is left
    /// to the caller, which knows the margin they stand at.
    pub fn wrap(&self, cells: usize) -> Vec<Line> {
        if cell_width(&self.text()) <= cells {
            return vec![self.clone()];
        }
        let mut lines = Vec::new();
        let mut line = self.empty_like();
        let mut used = 0;
        for (space, word) in self.words() {
            let width: usize = word.iter().map(|part| cell_width(&part.text)).sum();
            if used > 0 && used + 1 + width > cells {
                lines.push(std::mem::replace(&mut line, self.empty_like()));
                used = 0;
            }
            if used > 0 {
                line = line.push_with_fill(Tone::Plain, space, " ");
                used += 1;
            }
            line.spans.extend(word);
            used += width;
        }
        if used > 0 {
            lines.push(line);
        }
        lines
    }

    /// The line as rows inside `cells`, for a pane whose edge cuts what
    /// passes it: whole when it fits; else broken at the last space before
    /// the edge, or at the edge inside a word wider than what the row has
    /// left, each further row hung under where the line's text starts, and
    /// each part in the tone, fill and link of its span. Not [`Line::wrap`],
    /// which keeps a word whole past a stream's edge, where the terminal
    /// wraps it and a copy still takes it whole.
    pub fn rows_within(&self, cells: usize) -> Vec<Line> {
        let text = self.text();
        if cells == 0 || cell_width(&text) <= cells {
            return vec![self.clone()];
        }
        // The text starts past the marks, gutters and spaces that lead it.
        let lead = text.find(char::is_alphanumeric).unwrap_or(text.len());
        // A further row hangs under it while that leaves the row half the line.
        let hang = Some(cell_width(&text[..lead]))
            .filter(|width| *width <= cells / 2)
            .unwrap_or(0);
        let mut rows: Vec<Line> = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let (room, after) = match rows.is_empty() {
                true => (cells, lead),
                false => (cells - hang, start),
            };
            let (end, next) = row_end(&text, start, room, after);
            if end > start {
                let row = self.slice(start..end);
                rows.push(match rows.is_empty() {
                    true => row,
                    false => row.under(&" ".repeat(hang)),
                });
            }
            start = next;
        }
        rows
    }

    /// The line's words, in order, each as the parts of the spans it is made
    /// of, and the fill of the first whitespace before it. A word runs on
    /// across spans until whitespace ends it.
    fn words(&self) -> Vec<(Option<Fill>, Vec<Span>)> {
        let mut words: Vec<(Option<Fill>, Vec<Span>)> = Vec::new();
        let (mut in_word, mut space) = (false, None);
        for span in &self.spans {
            let mut part = String::new();
            for c in span.text.chars() {
                match (c.is_whitespace(), in_word) {
                    (true, _) => {
                        if let Some((_, word)) = words.last_mut().filter(|_| !part.is_empty()) {
                            word.push(Span {
                                text: std::mem::take(&mut part),
                                ..span.clone()
                            });
                        }
                        space = if in_word { span.fill } else { space };
                        in_word = false;
                    }
                    (false, false) => {
                        words.push((space, Vec::new()));
                        in_word = true;
                        part.push(c);
                    }
                    (false, true) => part.push(c),
                }
            }
            if let Some((_, word)) = words.last_mut().filter(|_| !part.is_empty()) {
                word.push(Span {
                    text: part,
                    ..span.clone()
                });
            }
        }
        words
    }

    /// An empty line that keeps this line's fill, to hold one of its pieces.
    fn empty_like(&self) -> Line {
        Line {
            spans: Vec::new(),
            fill: self.fill,
        }
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// The line with `other`'s spans after its own. The joined line keeps
    /// this line's fill, which paints all of it; a fill `other` has moves
    /// onto each of its spans, so they keep it.
    pub fn then(mut self, other: Line) -> Self {
        let fill = other.fill;
        self.spans.extend(other.spans.into_iter().map(|span| Span {
            fill: span.fill.or(fill),
            ..span
        }));
        self
    }

    /// What the line reads as, unpainted.
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// Where the row of `text` that starts at byte `start` ends inside `room`
/// cells, its trailing spaces left out, and where the next row starts, past
/// the spaces between them. It ends at its last space past byte `after`
/// when it has one, and else at the edge, one character at least, so every
/// row moves on.
fn row_end(text: &str, start: usize, room: usize, after: usize) -> (usize, usize) {
    let rest = &text[start..];
    let mut end = text.len();
    if cell_width(rest) > room {
        let (mut used, mut edge, mut space) = (0, start, None);
        for (at, c) in rest.char_indices() {
            used += cell_width(c.encode_utf8(&mut [0; 4]));
            if used > room {
                break;
            }
            edge = start + at + c.len_utf8();
            if c == ' ' && start + at > after {
                space = Some(start + at);
            }
        }
        let first = rest.chars().next().map_or(0, char::len_utf8);
        end = space.unwrap_or(edge.max(start + first));
    }
    let kept = start + text[start..end].trim_end().len();
    let next = text.len() - text[end..].trim_start().len();
    (kept, next)
}

/// The link to the file at `target`: a `file://` address with every byte of
/// the path but a letter, a digit, `-._~` and `/` written as `%` and its two
/// hex digits. A space or a `#` in a name would otherwise end the address
/// early, and a control character in it would end the link that carries it.
pub(crate) fn file_url(target: &std::path::Path) -> String {
    let mut uri = String::from("file://");
    for &byte in target.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

#[cfg(test)]
mod tests;

//! Markdown read as blocks, for its fences: which lines open and close one, as CommonMark reads
//! them, in list items too. A line not indented into an item ends it, and its fence, unless it
//! continues a paragraph lazily. Not read: fences on list marker lines, tabs as indentation,
//! HTML blocks, and block quote content beyond the paragraph a quoted line leaves open.

use std::ops::Range;

use super::Fence;

/// The most spaces a line may be indented past its container: four make it code.
const MAX_INDENT: usize = 3;

/// The fewest signs a thematic break has: a break is three or more of one
/// sign, so a shorter run is text.
const MIN_BREAK: usize = 3;

/// The most spaces after a list marker before its text: five or more make code.
const MAX_PAD: usize = 4;

/// The most digits an ordered list marker has: CommonMark allows nine, so a
/// longer number is text.
const MAX_DIGITS: usize = 9;

/// The fewest signs that open or close a fence: CommonMark opens and closes a
/// fence with three signs, so a shorter run is ordinary text.
pub(crate) const MIN_FENCE: usize = 3;

/// The run of signs a fence line starts with: a backtick or a tilde, how many
/// times it repeats, and how many spaces come before it.
#[derive(Clone, Copy)]
struct Run {
    sign: char,
    len: usize,
    indent: usize,
}

impl Run {
    /// The line that closes the fence this run opens: the run as it is written,
    /// indented as its opener is, so that a fence in a list item closes in it.
    fn closer(self) -> String {
        let run = self.sign.to_string().repeat(self.len);
        format!("{}{run}", " ".repeat(self.indent))
    }
}

/// A fence a line opened that no line has closed yet.
struct Opened {
    run: Run,
    opening: Range<usize>,
    info: Range<usize>,
    /// Where the line after its opening one starts.
    code: usize,
}

impl Opened {
    /// The fence `line`, which starts at `at`, opens with `run`; `next` is where the next line
    /// starts.
    fn at(line: &str, at: usize, next: usize, run: Run) -> Self {
        let marks = run.indent + run.len;
        let after = line.get(marks..).unwrap_or_default();
        let lead = after.len() - after.trim_start().len();
        let info = at + marks + lead;
        Opened {
            run,
            opening: at + run.indent..at + line.len(),
            info: info..info + after.trim().len(),
            code: next,
        }
    }

    /// This fence, its code ending on the line break before `at`, the line that closes or ends
    /// it, and what follows it starting at `next`.
    fn ending(self, closing: Option<Range<usize>>, at: usize, next: usize) -> Fence {
        Fence {
            opening: self.opening,
            info: self.info,
            code: self.code..at.saturating_sub(1).max(self.code),
            closing,
            indent: self.run.indent,
            next,
        }
    }
}

/// What the lines read so far leave open, and the fences they have ended.
#[derive(Default)]
struct Open {
    /// The fence a line opened that no line has closed yet, if any.
    fence: Option<Opened>,
    /// The content column of each open list item, outermost first.
    items: Vec<usize>,
    /// The open paragraph, if any: `Some(true)` when a block quote holds it.
    paragraph: Option<bool>,
    /// Whether the last line opened a list item with no text after its marker,
    /// so the next blank line ends that item.
    empty: bool,
    /// The fences read so far, in the order the text has them.
    fences: Vec<Fence>,
}

/// What a line starts, read in the container its content begins in.
enum Start {
    Fence(Run),
    /// A break; true when it is a block quote with text, which opens a paragraph.
    Break(bool),
    /// A list item, with its content column and whether text follows its marker.
    Item(usize, bool),
    /// Text: indented code when no paragraph is open and the line is indented four spaces or more.
    Text,
}

impl Open {
    /// Reads the line that starts at `at` and is followed by the line at `next`: a line of the
    /// fence being read, or a line outside one, read as a block.
    fn read(&mut self, line: &str, at: usize, next: usize) {
        if line.chars().all(|c| matches!(c, ' ' | '\t')) {
            // An item that starts with a blank line is empty, and ends there.
            if std::mem::take(&mut self.empty) {
                self.items.pop();
            }
            self.paragraph = None;
            return;
        }
        let body = line.trim_start_matches(' ');
        let indent = line.len() - body.len();
        if let Some(opened) = self.fence.take() {
            let column = self.items.last().copied().unwrap_or_default();
            if indent >= column {
                if closes(body, indent - column, opened.run) {
                    let marks = at + indent..at + line.len();
                    self.fences.push(opened.ending(Some(marks), at, next));
                } else {
                    self.fence = Some(opened);
                }
                return;
            }
            // Not indented into the item: the item ends, and the fence with it.
            self.fences.push(opened.ending(None, at, at));
        }
        self.block(line, indent, at, next);
    }

    /// Reads a line outside a fence, which starts at `at`: the items it is not indented into end,
    /// unless it continues a paragraph lazily.
    fn block(&mut self, line: &str, indent: usize, at: usize, next: usize) {
        let kept = self.items.partition_point(|&content| content <= indent);
        let popped = self.items.split_off(kept);
        let container = self.items.last().copied().unwrap_or_default();
        // A plain paragraph in the items the line is indented into restricts the
        // items that may start here; a quoted one, outside its quote, does not.
        let restricted = self.paragraph == Some(false) && popped.is_empty();
        let what = classify(line, container, restricted);
        self.empty = false;
        if matches!(what, Start::Text) && self.paragraph.is_some() {
            // Text after an open paragraph continues it, so no item ends.
            self.items.extend(popped);
            return;
        }
        self.paragraph = match what {
            Start::Fence(run) => {
                self.fence = Some(Opened::at(line, at, next, run));
                None
            }
            Start::Break(quoted) => quoted.then_some(true),
            Start::Item(content, text) => {
                self.items.push(content);
                self.empty = !text;
                text.then_some(false)
            }
            Start::Text => (indent - container <= MAX_INDENT).then_some(false),
        };
    }
}

/// What a text comes to as blocks: every fence in it, and the marks of one it ends inside.
pub(super) struct Reading {
    pub(super) fences: Vec<Fence>,
    left_open: Option<Run>,
}

/// `text`, read line by line.
pub(super) fn read(text: &str) -> Reading {
    let mut open = Open::default();
    let mut at = 0;
    for line in text.split('\n') {
        let next = (at + line.len() + 1).min(text.len());
        // A carriage return ends a CRLF line, so it is not part of the line.
        open.read(line.strip_suffix('\r').unwrap_or(line), at, next);
        at = next;
    }
    let left_open = open.fence.as_ref().map(|opened| opened.run);
    if let Some(opened) = open.fence.take() {
        // It runs on through the end of the text, with no line break before an ending line.
        let end = text.len();
        let code = opened.code..end;
        open.fences.push(Fence {
            code,
            ..opened.ending(None, end, end)
        });
    }
    Reading {
        fences: open.fences,
        left_open,
    }
}

/// The line that closes the fence `text` ends inside, when it ends inside one: the marks as
/// they are written, indented as its opener is, so that a fence in a list item closes in it.
pub(crate) fn fence_left_open(text: &str) -> Option<String> {
    read(text).left_open.map(Run::closer)
}

/// What a line starts in the container whose content begins at `container`.
/// A line indented more than three spaces past it is text.
fn classify(line: &str, container: usize, restricted: bool) -> Start {
    let body = line.trim_start_matches(' ');
    let indent = line.len() - body.len();
    if indent - container > MAX_INDENT {
        return Start::Text;
    }
    if let Some(run) = opening(body, indent) {
        return Start::Fence(run);
    }
    if crate::code::heading_level(body).is_some() || thematic(body) {
        return Start::Break(false);
    }
    if let Some(rest) = body.strip_prefix('>') {
        return Start::Break(!rest.trim().is_empty());
    }
    if restricted && setext(body) {
        return Start::Break(false);
    }
    item(body, indent, restricted).unwrap_or(Start::Text)
}

/// Whether a body is three or more of one of `-`, `*` or `_`, spaces apart or not.
fn thematic(body: &str) -> bool {
    let signs: Vec<char> = body.chars().filter(|c| !matches!(*c, ' ' | '\t')).collect();
    let first = signs.first().copied().unwrap_or(' ');
    signs.len() >= MIN_BREAK
        && matches!(first, '-' | '*' | '_')
        && signs.iter().all(|c| *c == first)
}

/// Whether a body is a setext underline: a run of `=` or of `-`, then spaces and tabs.
fn setext(body: &str) -> bool {
    let run = body.trim_end_matches([' ', '\t']);
    !run.is_empty() && (run.bytes().all(|b| b == b'=') || run.bytes().all(|b| b == b'-'))
}

/// The list item a body starts: its content column, and whether text follows
/// its marker. A restricted line needs text, and a bullet or a numbered one.
fn item(body: &str, indent: usize, restricted: bool) -> Option<Start> {
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let bullet = matches!(body.bytes().next(), Some(b'-' | b'+' | b'*'));
    let ordered = (1..=MAX_DIGITS).contains(&digits)
        && matches!(body.as_bytes().get(digits).copied(), Some(b'.' | b')'));
    if !bullet && !ordered {
        return None;
    }
    let marker = if bullet { 1 } else { digits + 1 };
    let after = body.get(marker..)?;
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    let rest = after.trim_start_matches(' ');
    let text = !rest.is_empty();
    let first = ordered && body.get(..digits).and_then(|n| n.parse::<usize>().ok()) == Some(1);
    if restricted && (!text || (ordered && !first)) {
        return None;
    }
    // Five or more spaces before the text make it code: the item then starts
    // one space past its marker, as an item without text does.
    let pad = after.len() - rest.len();
    let gap = if text && pad <= MAX_PAD { pad } else { 1 };
    Some(Start::Item(indent + marker + gap, text))
}

/// The fence a body opens at `indent`: a run of three or more signs, where a
/// backtick run has no backtick after it.
fn opening(body: &str, indent: usize) -> Option<Run> {
    let (sign, len, rest) = fence_run(body)?;
    (sign != '`' || !rest.contains('`')).then_some(Run { sign, len, indent })
}

/// Whether a body, `rel` spaces past its container, closes the fence `opened`.
fn closes(body: &str, rel: usize, opened: Run) -> bool {
    rel <= MAX_INDENT
        && fence_run(body).is_some_and(|(sign, len, rest)| {
            sign == opened.sign && len >= opened.len && rest.chars().all(|c| c == ' ' || c == '\t')
        })
}

/// The sign and length of the run a body starts with, and what follows the run,
/// when the run is three or more backticks or tildes.
fn fence_run(body: &str) -> Option<(char, usize, &str)> {
    let sign = body.chars().next().filter(|c| matches!(*c, '`' | '~'))?;
    let rest = body.trim_start_matches(sign);
    // A sign is one byte, so the bytes a run takes are the signs it counts.
    let len = body.len() - rest.len();
    (len >= MIN_FENCE).then_some((sign, len, rest))
}

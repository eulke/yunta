//! Prose as a terminal's Markdown reads it: its emphasis and its code spans
//! as tones, and the marks that make a line a list item or a heading.

use std::ops::Range;

use crate::line::{Line, Tone};

/// Prose as the spans it reads as: `**emphasis**` strong, its markers
/// dropped, and its code in a command's tone, its backticks kept in every
/// layer. Its code is what the lexer reads as Markdown's code spans, so a run
/// of backticks no run of as many closes is text. A `**` inside a code span
/// is the code's; the ones outside pair across it, so emphasis may hold
/// code, and a last `**` that none closes is text.
pub(in crate::surface) fn spans(text: &str) -> Line {
    let code = crate::code::code_spans(text);
    let marks = paired_marks(text, &code);
    let mut cuts: Vec<(Range<usize>, Cut)> = code
        .into_iter()
        .map(|span| (span, Cut::Code))
        .chain(marks.into_iter().map(|mark| (mark..mark + 2, Cut::Mark)))
        .collect();
    cuts.sort_by_key(|(range, _)| range.start);
    let mut line = Line::new();
    let mut strong = false;
    let mut at = 0;
    for (range, cut) in cuts {
        line = words(line, &text[at..range.start], strong);
        match cut {
            Cut::Code => line = line.push(Tone::Command, &text[range.clone()]),
            Cut::Mark => strong = !strong,
        }
        at = range.end;
    }
    words(line, &text[at..], strong)
}

/// Where prose changes what its words are: a code span, drawn in a
/// command's tone, or a `**` that opens or closes emphasis, dropped.
enum Cut {
    Code,
    Mark,
}

/// Where `text` holds a `**` outside its `code` spans, in pairs: a last
/// `**` that none closes is left out, to be read as text.
fn paired_marks(text: &str, code: &[Range<usize>]) -> Vec<usize> {
    let mut marks = Vec::new();
    let mut at = 0;
    let gaps = code
        .iter()
        .map(|span| (span.start, span.end))
        .chain([(text.len(), text.len())]);
    for (start, end) in gaps {
        let found = text[at..start].match_indices("**");
        marks.extend(found.map(|(offset, _)| at + offset));
        at = end;
    }
    if marks.len() % 2 == 1 {
        marks.pop();
    }
    marks
}

/// `text` onto `line`, strong where `strong` says.
fn words(line: Line, text: &str, strong: bool) -> Line {
    match strong {
        true => line.push(Tone::Strong, text),
        false => line.plain(text),
    }
}

/// The words of a list item or a heading from `at` on, read in its whole
/// `line`, so a run of backticks right after its marks is text unless a code
/// span closes it, as the lexer reads it.
pub(in crate::surface) fn marked_spans(line: &str, at: usize) -> Line {
    let read = spans(line);
    read.slice(at..read.text().len())
}

/// The words of a heading from `at` on: strong, its code spans in a command's
/// tone, read as a list item's are.
pub(super) fn heading_spans(line: &str, at: usize) -> Line {
    marked_spans(line, at)
        .spans()
        .iter()
        .fold(Line::new(), |words, span| {
            let tone = match span.tone {
                Tone::Command => Tone::Command,
                _ => Tone::Strong,
            };
            words.push(tone, span.text.as_str())
        })
}

/// A list item's marker and its text, or a heading, as a line of its own
/// with no marker, for a line that is one. A heading opens with one to six
/// `#` that a space, a tab or the line's end follows; any other line that
/// starts with `#` is prose.
pub(in crate::surface) fn list_item(line: &str) -> Option<(&str, &str)> {
    for mark in ["- ", "* ", "+ "] {
        if let Some(item) = line.strip_prefix(mark) {
            return Some((mark, item));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some(line.split_at(digits + 2));
    }
    let level = crate::code::heading_level(line)?;
    Some(("", line[level..].trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `line`'s spans, each as its text and its tone.
    fn read(line: &Line) -> Vec<(&str, Tone)> {
        line.spans()
            .iter()
            .map(|span| (span.text.as_str(), span.tone))
            .collect()
    }

    /// Emphasis reads as strong and a code span in a command's tone, its
    /// backticks kept; a lone backtick or a lone pair of stars is text.
    #[test]
    fn emphasis_and_code_are_tones_and_code_keeps_its_ticks() {
        let line = spans("run **this** with `yunta status`");
        assert_eq!(line.text(), "run this with `yunta status`");
        assert!(line
            .spans()
            .iter()
            .any(|span| span.tone == Tone::Strong && span.text == "this"));
        assert!(line
            .spans()
            .iter()
            .any(|span| span.tone == Tone::Command && span.text == "`yunta status`"));
        assert_eq!(spans("a lone ` tick").text(), "a lone ` tick");
        assert_eq!(spans("a lone ** star").text(), "a lone ** star");
    }

    /// A code span closes only at a run of as many backticks, so a shorter
    /// run inside it is its text.
    #[test]
    fn a_code_span_closes_only_at_a_run_as_long() {
        let line = spans("use ``a`b`` here");
        assert!(line
            .spans()
            .iter()
            .any(|span| span.tone == Tone::Command && span.text == "``a`b``"));
        assert_eq!(line.text(), "use ``a`b`` here");
    }

    /// Emphasis pairs its `**` across a code span, which keeps a command's
    /// tone inside it, and a `**` inside a code span is the code's.
    #[test]
    fn emphasis_holds_a_code_span_and_a_code_span_holds_its_stars() {
        let line = spans("**see `x` now** and `a**b`");
        assert_eq!(
            read(&line),
            [
                ("see ", Tone::Strong),
                ("`x`", Tone::Command),
                (" now", Tone::Strong),
                (" and ", Tone::Plain),
                ("`a**b`", Tone::Command),
            ]
        );
    }

    /// Marks pair in order, so where they are odd only the last is text.
    #[test]
    fn of_odd_marks_only_the_last_is_text() {
        let line = spans("**a** **b");
        assert_eq!(read(&line), [("a", Tone::Strong), (" **b", Tone::Plain)]);
    }
}

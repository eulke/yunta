//! Laying text out for a surface that has a shape to respect.
//!
//! A terminal row has one line, a Markdown bullet has an indent, and a
//! list of problems has a heading that counts them. None of that is a
//! property of what is being said, so it lives here rather than in the
//! types that say it — and once, rather than in each surface, because a
//! renderer that forgets the collapse does not produce a worse line, it
//! produces a broken table or an unparseable graph.

use std::fmt;

/// The width a rendered line stays inside.
///
/// Eighty cells is the floor a terminal is taken to have, and the width
/// a line still has to survive once it leaves the terminal — pasted
/// into a review, an issue, a log. A surface sizes its columns against
/// this and cuts what does not fit, because a wrap costs more than the
/// characters it would have dropped: it lands mid-column, and the table
/// a reader was scanning down stops being one.
///
/// Here rather than in the surface that draws, for the reason this
/// module exists: how wide a line may be is not a property of what is
/// being said, and a second surface deciding it again is a second
/// answer. That includes a test, which reads what a surface drew and
/// has to check it against the same number the surface used.
pub const LINE_WIDTH: usize = 80;

/// Whitespace collapsed to one line, for a place with room for exactly
/// one: a graph label, a row in a listing, a summary.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every line after the first prefixed, so a block hangs under the line
/// that introduced it and each line keeps the one it was written on.
pub fn hanging(text: &str, prefix: &str) -> String {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut out = first.to_string();
    for line in lines {
        out.push('\n');
        out.push_str(prefix);
        out.push_str(line);
    }
    out
}

/// Every line prefixed, the first one included, for a surface that
/// nests the whole block rather than introducing it.
pub fn indent(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A headline and the detail that explains it, joined by `": "`, and the
/// headline alone when the detail says nothing.
///
/// A colon promises a reader that something follows it, so nothing
/// promises it when a source of detail — a subprocess that wrote no
/// stderr, a failure whose cause carries no message — came back empty.
/// Surrounding whitespace is not detail either: `"  \n"` reads as
/// nothing said.
///
/// ```
/// # use yunta_core::text::detailed;
/// assert_eq!(detailed("exit 1", "no such file"), "exit 1: no such file");
/// assert_eq!(detailed("exit 1", ""), "exit 1");
/// ```
pub fn detailed(headline: impl fmt::Display, detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        return headline.to_string();
    }
    format!("{headline}: {detail}")
}

/// A subject and what it carries, set off by an em dash, and the subject
/// alone when there is nothing to set off.
///
/// The dash promises a reader exactly what [`detailed`]'s colon does;
/// what chooses between them is the shape of what follows. A colon
/// introduces a value, so it reads wrong in front of something that
/// carries colons of its own — a claim followed by the labelled facts
/// behind it, or an event line followed by the free text a payload
/// holds. The dash sets those aside instead of introducing them.
///
/// ```
/// # use yunta_core::text::aside;
/// assert_eq!(aside("run_paused", "cap: 400; spent: 500"), "run_paused — cap: 400; spent: 500");
/// assert_eq!(aside("run_paused", ""), "run_paused");
/// ```
pub fn aside(subject: impl fmt::Display, carried: &str) -> String {
    let carried = carried.trim();
    if carried.is_empty() {
        return subject.to_string();
    }
    format!("{subject} — {carried}")
}

/// The block `spec-ledger.md` §4 fixes: a heading naming what was read
/// and how many problems it has, then one indented line per problem.
///
/// ```text
/// workflows/ship.yaml: 2 errors
///   the document: unknown key `version`; the only top-level key is `nodes`
///   node `plan`: `runner` is empty
/// ```
///
/// Every surface that lists what is wrong with one file goes through
/// here, so a reader meets the same block whether a workflow failed to
/// check or an artifact failed to close.
pub fn problems(heading: impl fmt::Display, items: &[impl fmt::Display]) -> String {
    let mut text = format!(
        "{heading}: {} {}",
        items.len(),
        if items.len() == 1 { "error" } else { "errors" }
    );
    for item in items {
        text.push_str(&format!("\n  {item}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{aside, detailed};

    #[test]
    fn a_headline_with_detail_is_joined_by_a_colon() {
        assert_eq!(
            detailed("exit 1", "cannot open `x`"),
            "exit 1: cannot open `x`"
        );
    }

    #[test]
    fn a_headline_whose_detail_is_empty_keeps_no_colon_promising_one() {
        assert_eq!(detailed("exit 1", ""), "exit 1");
    }

    #[test]
    fn whitespace_is_not_detail() {
        assert_eq!(detailed("exit 1", "  \n\t "), "exit 1");
    }

    #[test]
    fn detail_keeps_its_own_lines_and_loses_only_its_margins() {
        assert_eq!(
            detailed("exit 2", "\nfirst\nsecond\n"),
            "exit 2: first\nsecond"
        );
    }

    #[test]
    fn a_subject_with_something_to_carry_is_joined_by_a_dash() {
        assert_eq!(
            aside("run_paused", "cap: 400; spent: 500"),
            "run_paused — cap: 400; spent: 500"
        );
    }

    #[test]
    fn a_subject_carrying_nothing_keeps_no_dash_promising_one() {
        assert_eq!(aside("run_paused", ""), "run_paused");
        assert_eq!(aside("run_paused", "  \n\t "), "run_paused");
    }
}

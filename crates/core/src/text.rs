//! Laying text out for a surface that has a shape to respect.
//!
//! A terminal row has one line, a Markdown bullet has an indent, and a
//! list of problems has a heading that counts them. None of that is a
//! property of what is being said, so it lives here rather than in the
//! types that say it — and once, rather than in each surface, because a
//! renderer that forgets the collapse does not produce a worse line, it
//! produces a broken table or an unparseable graph.

use std::fmt;

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

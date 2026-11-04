//! Reading the shape of a source file, for the counters that measure it.
//!
//! A ratchet that counts by line has to know which lines are which: the
//! ones inside a `#[cfg(test)]` module, the ones inside a string or a
//! comment where a brace means nothing, and the span from a function's
//! opening brace to its matching close. These are deliberate heuristics
//! rather than a parser: each only ever gates *growth* against a
//! baseline, so a stable miscount costs nothing and a wrong answer about
//! one line never changes a verdict about the tree.

use surco::code::items::{Kind, Text};
use surco::code::Language;

/// A source file with its inline `#[cfg(test)]` modules blanked, so a
/// production counter never sees the unit tests that live beside the code:
/// these patterns are measured *outside* tests.
pub fn production_only(text: &str) -> String {
    let mask = test_mod_mask(text);
    text.lines()
        .zip(mask)
        .map(|(line, in_test)| {
            if in_test {
                String::new()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every line of `source`, read as Rust, with its comments and the text
/// of its literals turned to spaces, so a brace inside a string or a
/// comment never opens or closes a body. A literal or a block comment
/// that runs over lines runs over them here too.
pub(super) fn code_lines(source: &str) -> Vec<String> {
    surco::code::masked(source, Some(surco::code::Language::Rust))
        .lines()
        .map(str::to_string)
        .collect()
}

/// One flag per line: whether it sits inside a `#[cfg(test)]` module (the
/// attribute line, the `mod … {` header, and the body through its matching
/// `}`). Braces inside strings and comments are ignored via [`code_lines`],
/// so a `{` in a string never opens a phantom module.
pub fn test_mod_mask(text: &str) -> Vec<bool> {
    let lines: Vec<&str> = text.lines().collect();
    let blanked = code_lines(text);
    let mut mask = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if blanked[i].contains("#[cfg(test)]") {
            // Find the block this attribute guards (a `mod`) and its opening
            // brace; a `#[cfg(test)]` on anything else is left alone.
            let mut j = i;
            let mut is_mod = false;
            while j < lines.len() {
                if blanked[j].contains(" mod ") || blanked[j].trim_start().starts_with("mod ") {
                    is_mod = true;
                }
                if blanked[j].contains('{') {
                    break;
                }
                if blanked[j].contains(';') {
                    break;
                }
                j += 1;
            }
            if is_mod && j < lines.len() && blanked[j].contains('{') {
                // A module whose braces never balance runs to the end of the
                // file: a `/*` that opens a comment and never closes hides the
                // braces after it.
                let mut depth = 0i32;
                let mut close = lines.len() - 1;
                for (k, line) in blanked.iter().enumerate().skip(j) {
                    depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                    if depth == 0 {
                        close = k;
                        break;
                    }
                }
                for flag in mask.iter_mut().take(close + 1).skip(i) {
                    *flag = true;
                }
                i = close + 1;
                continue;
            }
        }
        i += 1;
    }
    mask
}

/// Whether `line` carries a string literal with eight or more spaces
/// between two visible characters — the trace a `\` line continuation
/// leaves when it goes missing: the message reaches its reader with a
/// source line's indentation inside it, never shallower than two
/// nesting levels. Alignment an author writes into a message (a table
/// column, a commented YAML template) uses a few spaces, and spaces a
/// literal opens with or that follow a `\n` escape are layout; comment
/// lines are not messages.
pub fn has_inner_space_run(line: &str) -> bool {
    if line.trim_start().starts_with("//") {
        return false;
    }
    let Some(open) = line.find('"') else {
        return false;
    };
    let bytes = line.as_bytes();
    let mut i = open + 1;
    while let Some(&byte) = bytes.get(i) {
        if byte != b' ' {
            i += 1;
            continue;
        }
        let start = i;
        while bytes.get(i) == Some(&b' ') {
            i += 1;
        }
        let before = start.checked_sub(1).and_then(|j| bytes.get(j));
        let after_escape = start >= 2 && bytes.get(start - 2..start) == Some(b"\\n".as_slice());
        let visible_before = before.is_some_and(|&b| b != b'"' && b != b'\\');
        let visible_after = bytes.get(i).is_some_and(|&b| b != b'"');
        if i - start >= 8 && visible_before && !after_escape && visible_after {
            return true;
        }
    }
    false
}

/// Whether `line` says an em or en dash outside a comment: a sentence that
/// joins its parts with one instead of the punctuation every reader reads.
pub fn has_dash(line: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with("//") {
        return false;
    }
    let code = line.split(" // ").next().unwrap_or(line);
    code.contains(['\u{2014}', '\u{2013}'])
}

/// Whether `line` is a doc comment that says an em or en dash: where clap
/// prints a command's doc comments as its help, they are sentences a reader
/// is told.
pub fn has_doc_dash(line: &str) -> bool {
    line.trim_start().starts_with("///") && line.contains(['\u{2014}', '\u{2013}'])
}

/// Whether `line` hedges a count with a plural in parentheses —
/// `3 file(s)`, `2 child(ren)`, `a process(es)` — where the number beside
/// it already says how many: a word written just before a parenthesis
/// holding a plural's ending, after a space or at the start of a
/// continued line, which is prose and never a call (`Some(s)`,
/// `str::trim(s)`). Comment lines are not messages.
pub fn has_parenthesized_plural(line: &str) -> bool {
    if line.trim_start().starts_with("//") {
        return false;
    }
    line.match_indices('(').any(|(at, _)| {
        let after = &line[at + 1..];
        let plural = ["s)", "es)", "ies)", "ren)"]
            .iter()
            .any(|ending| after.starts_with(ending));
        if !plural {
            return false;
        }
        let before = &line[..at];
        let word = before
            .trim_end_matches(|c: char| c.is_ascii_lowercase() || c == '-')
            .len();
        word < at
            && before[..word]
                .chars()
                .last()
                .is_none_or(char::is_whitespace)
    })
}

/// How many function bodies in `source` exceed `max` lines, measured from
/// the line after the body's opening `{` to its matching `}` — the span a
/// reader scrolls. The bodies are the ones the language's reader finds
/// (`surco::code::items`), so a brace inside a string or a comment never
/// opens or closes a body. A deliberate heuristic, not a parser: it only
/// ever gates *growth* against a baseline, so an occasional miscount is
/// stable and harmless.
pub fn functions_over(source: &str, max: usize) -> usize {
    bodies(source)
        .iter()
        .filter(|body| body.lines() > max)
        .count()
}

/// Counts `std::fs::` calls inside an `async fn` body, except those covered
/// by a `// blocking:` line directly above or by the argument list of
/// `spawn_blocking(`.
///
/// A synchronous file call blocks the thread the runtime gave the task:
/// every other task sharing that thread waits for the disk, and nothing
/// in the code says so. Disk work inside an async body goes through the
/// async API or through `spawn_blocking`, or carries the reason a
/// `// blocking:` comment gives.
pub fn sync_fs_in_async(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    let blanked = code_lines(source);
    bodies(source)
        .iter()
        .filter(|body| blanked[body.header].contains("async fn"))
        .map(|body| {
            let mut open = 0i64; // parentheses open around a `spawn_blocking(` call
            let mut count = 0;
            for at in body.open + 1..body.close.min(blanked.len()) {
                let code = &blanked[at];
                let offload = code.find("spawn_blocking(");
                let disk = code.find("std::fs::");
                let offloaded = open > 0 || matches!((offload, disk), (Some(o), Some(d)) if d > o);
                let justified = at > 0 && lines[at - 1].trim_start().starts_with("// blocking:");
                if disk.is_some() && !offloaded && !justified {
                    count += 1;
                }
                if open > 0 || offload.is_some() {
                    let code = &code[offload.unwrap_or(0)..];
                    open = (open + code.matches('(').count() as i64
                        - code.matches(')').count() as i64)
                        .max(0);
                }
            }
            count
        })
        .sum()
}

/// The span of a function body: the line its `fn` header opens, the line
/// its body's `{` opens, and the line its matching `}` closes.
struct Body {
    header: usize,
    open: usize,
    close: usize,
}

impl Body {
    /// How many lines a reader scrolls between the braces.
    fn lines(&self) -> usize {
        self.close.saturating_sub(self.open + 1)
    }
}

/// Every function body in `source`, in source order: what the language's reader finds at the
/// top, in a module, an `impl` or a trait. A function nested inside another is part of the body
/// that holds it, so each span is reported once, by its outermost function.
fn bodies(source: &str) -> Vec<Body> {
    let Some(text) = Text::of(source, Language::Rust) else {
        return Vec::new();
    };
    let line_of = |at: usize| source[..at.min(source.len())].matches('\n').count();
    let mut out = Vec::new();
    let mut held = text.items();
    while let Some(item) = held.pop() {
        match (item.kind, &item.bounds.body) {
            (Kind::Fn, Some(body)) => out.push(Body {
                header: line_of(item.head),
                open: line_of(item.bounds.stop),
                close: line_of(body.end),
            }),
            (kind, Some(_)) if kind.holds_members() => held.extend(text.inside(&item)),
            _ => {}
        }
    }
    out.sort_by_key(|body| body.header);
    out
}

/// Whether `line` formats a value with its `Debug` form in text that
/// reaches a reader — a person or an agent. `Debug` is the shape Rust
/// gives a value for whoever wrote it: `Some(..)`, `TaskRecord { .. }`,
/// `Major`. An assertion's, a panic's or a trace's message is for the
/// one who wrote the code, and is not counted.
pub fn has_debug_format(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") || !(code.contains(":?}") || code.contains(":#?}")) {
        return false;
    }
    ![
        "assert",
        "panic!",
        "unreachable!",
        "debug!",
        "trace!",
        "tracing::",
        "todo!",
    ]
    .iter()
    .any(|developer| code.contains(developer))
}

#[cfg(test)]
mod tests;

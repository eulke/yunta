//! How text is escaped for Markdown: words keep a sign HTML would read as a tag
//! escaped and never escape or pair with what is written after them, text read
//! as written keeps every sign Markdown reads, a code span keeps its text as
//! written, and a table cell stays on one line.

/// Text for one table cell: on one line, as `plain_text` writes it, and with
/// the pipe that would end the cell escaped.
pub(super) fn cell(text: &str) -> String {
    let plain = plain_text(&crate::text::one_line(text));
    let mut out = String::with_capacity(plain.len());
    for (at, part) in plain.split('|').enumerate() {
        if at > 0 {
            push_escape(&mut out, "\\|");
        }
        out.push_str(part);
    }
    out
}

/// Text for one table cell as a code span: on one line, with each pipe
/// escaped, since a table ends its cell at a pipe even inside a span.
pub(super) fn cell_code_span(text: &str) -> String {
    code_span(&crate::text::one_line(text).replace('|', "\\|"))
}

/// `text` as a code span, kept as written: fenced by a run of backticks one
/// longer than the longest run in `text`, so no run inside closes it, and
/// padded by a space each side when `text` needs one.
pub(super) fn code_span(text: &str) -> String {
    let fence = "`".repeat(longest_run(text) + 1);
    let pad = match needs_pad(text) {
        true => " ",
        false => "",
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// Whether a code span of `text` needs a space each side: a backtick at an end
/// would run into the fence, and a space at each end, with text between them,
/// would lose one to Markdown.
fn needs_pad(text: &str) -> bool {
    let spaced = text.starts_with(' ') && text.ends_with(' ') && !text.trim_matches(' ').is_empty();
    text.starts_with('`') || text.ends_with('`') || spaced
}

/// The longest run of backticks in `text`, zero when it has none.
fn longest_run(text: &str) -> usize {
    text.split(|sign: char| sign != '`')
        .map(str::len)
        .max()
        .unwrap_or_default()
}

/// `text` as Markdown says it as written, with a backslash before each sign
/// Markdown would read as markup inside a line. A run of underscores
/// between two letters or digits opens and closes nothing, so a name or a
/// path keeps the ones it is most often written with.
pub(super) fn literal(text: &str) -> String {
    let signs: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (at, &sign) in signs.iter().enumerate() {
        let inert = sign == '_' && joined(&signs, at);
        if matches!(sign, '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' | '&') && !inert {
            out.push('\\');
        }
        out.push(sign);
    }
    out
}

/// `text` as plain text, read as written. Each `<` gets a backslash before it,
/// so no sign opens a tag. A backslash that ends the words, ends a line or comes
/// before a `<` is doubled, so it escapes nothing written after it. A code span
/// is a run of backticks closed by a run of the same length, and keeps its text
/// as written, line ends included; a run that nothing closes has a backslash
/// before each of its backticks, so it cannot pair with a backtick written
/// beside it.
pub(super) fn plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('`') {
        push_words(&mut out, &rest[..at]);
        rest = &rest[at..];
        let run = leading_backticks(rest);
        match close(rest, run) {
            Some(end) => {
                double_held(&mut out);
                out.push_str(&rest[..end]);
                rest = &rest[end..];
            }
            None => {
                for _ in 0..run {
                    push_escape(&mut out, "\\`");
                }
                rest = &rest[run..];
            }
        }
    }
    push_words(&mut out, rest);
    double_held(&mut out);
    out
}

/// Writes `words`, text outside a code span, to `out` as `plain_text` reads
/// it: each `<` is escaped, and a line end is written after the backslashes
/// before it are doubled, so a backslash at the end of a line escapes nothing.
fn push_words(out: &mut String, words: &str) {
    for sign in words.chars() {
        match sign {
            '<' => push_escape(out, "\\<"),
            '\r' | '\n' => {
                double_held(out);
                out.push(sign);
            }
            _ => out.push(sign),
        }
    }
}

/// Doubles the backslashes `out` ends with, so Markdown reads each as a
/// backslash of its own. What is written after them, a ** closing bold, a
/// backtick opening a code span or the end of the words, would otherwise
/// escape them or read them as a line break.
fn double_held(out: &mut String) {
    let held = out.len() - out.trim_end_matches('\\').len();
    out.push_str(&"\\".repeat(held));
}

/// Writes `escape`, the backslash and sign that keep a sign as written, to
/// `out`. The backslashes `out` already ends with are doubled first, so
/// Markdown reads each as a backslash of its own, and the sign after them as
/// escaped, rather than reading the pair as the start of a tag or a cell end.
fn push_escape(out: &mut String, escape: &str) {
    double_held(out);
    out.push_str(escape);
}

/// Whether the underscore at `at` in `signs` sits between two letters or
/// digits, where Markdown opens and closes nothing with it.
fn joined(signs: &[char], at: usize) -> bool {
    let worded = |beside: Option<&char>| beside.is_some_and(|sign| sign.is_alphanumeric());
    worded(signs[..at].iter().rev().find(|before| **before != '_'))
        && worded(signs[at..].iter().find(|after| **after != '_'))
}

/// Where the code span that `rest` opens with `run` backticks ends: after the
/// next run of as many backticks, or `None` when no run closes it.
fn close(rest: &str, run: usize) -> Option<usize> {
    let mut from = run;
    while let Some(found) = rest[from..].find('`') {
        let start = from + found;
        let len = leading_backticks(&rest[start..]);
        if len == run {
            return Some(start + len);
        }
        from = start + len;
    }
    None
}

/// How many backticks `text` begins with.
fn leading_backticks(text: &str) -> usize {
    text.len() - text.trim_start_matches('`').len()
}

/// `text` as the lines of one item of a list or a quote: its first line after
/// `first`, each later line after `rest`, and a blank line written as its
/// prefix with the trailing spaces trimmed, so a quote keeps its `>` on a
/// blank line and no blank line ends in a space.
pub(super) fn item(first: &str, rest: &str, text: &str) -> String {
    let mut lines = text.lines();
    let head = lines.next().unwrap_or_default();
    std::iter::once(prefixed(first, head))
        .chain(lines.map(|line| prefixed(rest, line)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `line` after `prefix`, or `prefix` alone with its trailing spaces trimmed
/// when `line` is blank.
fn prefixed(prefix: &str, line: &str) -> String {
    match line.trim().is_empty() {
        true => prefix.trim_end().to_string(),
        false => format!("{prefix}{line}"),
    }
}

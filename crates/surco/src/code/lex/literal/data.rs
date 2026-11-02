//! How shell, YAML, TOML, JSON and Markdown write their comments and their
//! literals. In these a quoted word is the same word unquoted, so a masked
//! file keeps their literals' text and blanks only their comments.
//! Markdown's are read in [`markdown`](mod@markdown).

use super::{bytes_from, is_word, line_end, quoted, run, tripled, Escape, Holes, Literal};

mod markdown;

pub(super) use markdown::markdown;
pub(crate) use markdown::Fence;

/// Where the `#` comment that starts at `at` ends, if one does, in shell or
/// YAML: a `#` opens one only where a word could start, since one inside a
/// word, as in `a#b` or a link's `#part`, is part of the word.
pub(super) fn word_comment(source: &[u8], at: usize) -> Option<usize> {
    (source.get(at) == Some(&b'#') && word_starts(source, at)).then(|| line_end(source, at))
}

/// Where the Markdown comment that starts at `at` ends, if one does: an HTML
/// comment, `<!-- … -->`, through its close or to the source's end.
pub(super) fn markdown_comment(source: &[u8], at: usize) -> Option<usize> {
    bytes_from(source, at).starts_with(b"<!--").then(|| {
        let after = at + 4;
        bytes_from(source, after)
            .windows(3)
            .position(|window| window == b"-->")
            .map_or(source.len(), |found| after + found + 3)
    })
}

/// The shell literal that starts at `at`, if one does, each running on over
/// lines: a single quote, whose text escapes nothing unless a `$` opens it;
/// a double quote, which escapes with `\` and holds `${…}`, `$(…)` and
/// `$name`; or a here-document. A quote a `\` escapes opens nothing.
pub(super) fn shell(source: &[u8], at: usize) -> Option<Literal> {
    match bytes_from(source, at) {
        [b'\'' | b'"' | b'<', ..] if escaped(source, at) => None,
        [b'\'', ..] => {
            let before = at.checked_sub(1).and_then(|before| source.get(before));
            let escape = match before {
                Some(b'$') => Escape::Backslash,
                _ => Escape::None,
            };
            Some(Literal::new(at + 1, b"'", escape, true, Holes::None))
        }
        [b'"', ..] => Some(Literal::new(
            at + 1,
            b"\"",
            Escape::Backslash,
            true,
            Holes::Shell,
        )),
        [b'<', b'<', ..] => heredoc(source, at),
        _ => None,
    }
}

/// The here-document whose operator, `<<WORD` or `<<-WORD`, starts at `at`.
/// The rest of the operator's line is code; the document's text is the
/// lines after it, through the line that holds the word alone (after its
/// tabs, for `<<-`), or to the source's end. Its text holds `${…}`, `$(…)`
/// and `$name` unless the word is quoted or escaped, as in `<<'EOF'`. A
/// `<<<` passes a string, not a document, and a `<<` inside arithmetic,
/// `$((…))` or `((…))`, shifts bits.
fn heredoc(source: &[u8], at: usize) -> Option<Literal> {
    if source.get(at + 2) == Some(&b'<') || in_arithmetic(source, at) {
        return None;
    }
    let dash = source.get(at + 2) == Some(&b'-');
    let opens = at + 2 + usize::from(dash);
    let quote_at = opens + run(source, opens, |byte| byte == b' ' || byte == b'\t');
    let quote = source
        .get(quote_at)
        .copied()
        .filter(|byte| matches!(byte, b'\'' | b'"' | b'\\'));
    let start = quote_at + usize::from(quote.is_some());
    let length = run(source, start, is_word);
    let word = source
        .get(start..start + length)
        .filter(|word| !word.is_empty())?;
    let closed =
        quote.is_some_and(|quote| quote != b'\\' && source.get(start + length) == Some(&quote));
    let rest = start + length + usize::from(closed);
    let mut line = line_end(source, rest) + 1;
    let end = loop {
        if line > source.len() {
            break source.len();
        }
        let close = line_end(source, line);
        let said = source.get(line..close).unwrap_or_default();
        let said = said.strip_suffix(b"\r").unwrap_or(said);
        let tabs = match dash {
            true => said.iter().take_while(|&&byte| byte == b'\t').count(),
            false => 0,
        };
        if said.get(tabs..) == Some(word) {
            break close;
        }
        line = close + 1;
    };
    let holes = Holes::Heredoc {
        line: rest,
        expands: quote.is_none(),
    };
    Some(Literal::ending(rest, end, holes))
}

/// What a shell line holds open at a point, as the line is read from its
/// start: a group or a command substitution, arithmetic, or a double quote.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A `(` that groups, or a `$(` that substitutes a command.
    Group,
    /// A `((`, or a `$((`, that opens arithmetic until its `))`.
    Arithmetic,
    /// A double quote, whose text holds a `$((` or a `$(` as code.
    Quote,
}

/// Whether `at` stands inside shell arithmetic, `$((…))` or `((…))`, that
/// opened earlier on its line and has not closed there. The line is read
/// from its start: quotes hide a `((` unless it is a `$((` in double quotes,
/// and a group's `)` closes the group, not the arithmetic around it.
fn in_arithmetic(source: &[u8], at: usize) -> bool {
    let line = source.get(line_start(source, at)..at).unwrap_or_default();
    let mut frames = Vec::new();
    let mut index = 0;
    while index < line.len() {
        index += step(&mut frames, line, index);
    }
    frames.contains(&Frame::Arithmetic)
}

/// Reads the bytes at `index` of `line` as the innermost open frame takes
/// them: a frame they open or close, or text a frame holds. Answers how many
/// bytes they span, at least one.
fn step(frames: &mut Vec<Frame>, line: &[u8], index: usize) -> usize {
    let rest = bytes_from(line, index);
    let top = frames.last().copied();
    match (top, rest) {
        (Some(Frame::Quote), [b'\\', ..]) => 2,
        (Some(Frame::Quote), [b'"', ..]) => {
            frames.pop();
            1
        }
        (Some(Frame::Quote), [b'$', b'(', b'(', ..]) => {
            frames.push(Frame::Arithmetic);
            3
        }
        (Some(Frame::Quote), [b'$', b'(', ..]) => {
            frames.push(Frame::Group);
            2
        }
        (Some(Frame::Quote), _) => 1,
        (_, [b'\\', ..]) => 2,
        (_, [b'\'', ..]) => single_quote(line, index),
        (_, [b'"', ..]) => {
            frames.push(Frame::Quote);
            1
        }
        (_, [b'(', b'(', ..]) => {
            frames.push(Frame::Arithmetic);
            2
        }
        (_, [b'(', ..]) => {
            frames.push(Frame::Group);
            1
        }
        (Some(Frame::Arithmetic), [b')', b')', ..]) => {
            frames.pop();
            2
        }
        (Some(Frame::Group | Frame::Arithmetic), [b')', ..]) => {
            frames.pop();
            1
        }
        _ => 1,
    }
}

/// How many bytes the single quote at `index` of `line` spans: through the
/// next quote, or to the line's end when none closes it. A quote right after
/// a `$` is ANSI-C's, where a backslash escapes the byte after it.
fn single_quote(line: &[u8], index: usize) -> usize {
    let escapes = index > 0 && line.get(index - 1) == Some(&b'$');
    let mut span = 1;
    while let Some(&byte) = line.get(index + span) {
        span += 1;
        match byte {
            b'\\' if escapes => span += 1,
            b'\'' => break,
            _ => {}
        }
    }
    span
}

/// The YAML literal that starts at `at`, if one does, where a value or a key
/// starts, each running on over lines: a single quote, whose text writes a
/// quote as `''`; a double quote, which escapes with `\`; or a block scalar.
pub(super) fn yaml(source: &[u8], at: usize) -> Option<Literal> {
    match source.get(at)? {
        b'\'' if value_starts(source, at) => Some(Literal::new(
            at + 1,
            b"'",
            Escape::Doubled,
            true,
            Holes::None,
        )),
        b'"' if value_starts(source, at) => Some(Literal::new(
            at + 1,
            b"\"",
            Escape::Backslash,
            true,
            Holes::None,
        )),
        b'|' | b'>' if value_starts(source, at) => block_scalar(source, at),
        _ => None,
    }
}

/// The block scalar whose indicator, `|` or `>`, stands at `at`: the
/// indicator with its chomping and indentation signs, and the lines under
/// it indented as far as the first of them, and further than the
/// indicator's own line, blank lines among them, through the last such
/// line. An indicator that anything but a comment follows on its line
/// opens none.
fn block_scalar(source: &[u8], at: usize) -> Option<Literal> {
    let signs = at
        + 1
        + run(source, at + 1, |byte| {
            matches!(byte, b'+' | b'-' | b'0'..=b'9')
        });
    let after = signs + run(source, signs, |byte| byte == b' ' || byte == b'\t');
    if !matches!(source.get(after), None | Some(b'\n' | b'\r' | b'#')) {
        return None;
    }
    let own = indent(source, line_start(source, at));
    let mut end = line_end(source, at);
    let mut width = None;
    let mut line = end + 1;
    while line < source.len() {
        let close = line_end(source, line);
        let blank = source
            .get(line..close)
            .unwrap_or_default()
            .iter()
            .all(u8::is_ascii_whitespace);
        if !blank {
            let depth = indent(source, line);
            let width = *width.get_or_insert(depth);
            if width <= own || depth < width {
                break;
            }
            end = close;
        }
        line = close + 1;
    }
    Some(Literal::ending(at + 1, end, Holes::None))
}

/// The TOML string that starts at `at`, if one does: `"…"`, which escapes
/// with `\`, and `'…'`, which escapes nothing, each kept to its line; or
/// either tripled, which runs on over lines.
pub(super) fn toml(source: &[u8], at: usize) -> Option<Literal> {
    match source.get(at)? {
        b'"' => tripled(source, at, b'"', Escape::Backslash, Holes::None)
            .or_else(|| quoted(source, at, b'"', false, Holes::None)),
        b'\'' => tripled(source, at, b'\'', Escape::None, Holes::None)
            .or_else(|| Some(Literal::new(at + 1, b"'", Escape::None, false, Holes::None))),
        _ => None,
    }
}

/// The JSON string that starts at `at`, if one does: `"…"`, kept to its
/// line.
pub(super) fn json(source: &[u8], at: usize) -> Option<Literal> {
    quoted(source, at, b'"', false, Holes::None)
}

/// Whether a word could start at `at`: at the start of the source or of a
/// line, or after a space or a tab.
fn word_starts(source: &[u8], at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|before| source.get(before))
        .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'\n'))
}

/// Whether a `\` escapes the byte at `at`: an odd number of them stands
/// right before it.
fn escaped(source: &[u8], at: usize) -> bool {
    let backslashes = source
        .get(..at)
        .unwrap_or_default()
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    backslashes % 2 == 1
}

/// Whether a YAML value or key could start at `at`: nothing but spaces and
/// tabs stands before it on its line, or the last sign before it opens a
/// value: `:`, `-`, `?`, `,`, `[` or `{`. A quote inside a plain scalar, as
/// in `don't`, opens nothing. It looks back only over the spaces before
/// `at`, so a long line costs no more for each quote in it than a short one.
fn value_starts(source: &[u8], at: usize) -> bool {
    let before = source.get(..at).unwrap_or_default();
    match before
        .iter()
        .rev()
        .find(|&&byte| byte != b' ' && byte != b'\t')
    {
        None | Some(b'\n') => true,
        Some(sign) => matches!(sign, b':' | b'-' | b'?' | b',' | b'[' | b'{'),
    }
}

/// Where the line holding `at` starts.
fn line_start(source: &[u8], at: usize) -> usize {
    source
        .get(..at)
        .unwrap_or_default()
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |newline| newline + 1)
}

/// How many spaces open the line that starts at `line`.
fn indent(source: &[u8], line: usize) -> usize {
    run(source, line, |byte| byte == b' ')
}

//! The literals of the programming languages that go past a plain quote:
//! Rust's raw strings and chars, C++'s raw strings, Scala's interpolators,
//! Swift's raw and tripled strings, C#'s verbatim, raw and interpolated
//! strings, Dart's raw and tripled strings, and Python's prefixes and tripled
//! strings, and a script's regular expressions. The dispatch in the parent
//! module picks one by language; the plain quotes are read there.

use super::{
    after_word, bytes_from, char_literal, comment, is_word, quoted, run, tripled, width, Escape,
    Holes, Language, Literal,
};

/// Rust's strings, `"…"`, raw `r#"…"#`, byte and C ones, and its chars.
pub(super) fn rust(source: &[u8], at: usize) -> Option<Literal> {
    let starts = !after_word(source, at);
    let prefixed = starts && matches!(source.get(at), Some(b'b' | b'c'));
    let from = at + usize::from(prefixed);
    if starts && source.get(from) == Some(&b'r') {
        let hashes = run(source, from + 1, |byte| byte == b'#');
        let open = from + 1 + hashes;
        if source.get(open) == Some(&b'"') {
            let mut close = vec![b'"'];
            close.extend(std::iter::repeat_n(b'#', hashes));
            return Some(Literal::new(
                open + 1,
                &close,
                Escape::None,
                true,
                Holes::None,
            ));
        }
    }
    quoted(source, from, b'"', true, Holes::None).or_else(|| rust_char(source, from))
}

/// A Rust char at `at`: an escape, as [`char_literal`] reads one, or one
/// char between two quotes. A lifetime is a quote one char past which none
/// stands, however near the next: `<'a>(&'a self` opens no char.
fn rust_char(source: &[u8], at: usize) -> Option<Literal> {
    let body = source
        .get(at + 1..)
        .filter(|_| source.get(at) == Some(&b'\''))?;
    if body.first() == Some(&b'\\') {
        return char_literal(source, at);
    }
    // The fewest bytes that read as a char are one char's.
    let width = (1..=4).find(|&end| {
        body.get(..end)
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_ok())
    })?;
    (body.get(width) == Some(&b'\''))
        .then(|| Literal::new(at + 1, b"'", Escape::Backslash, false, Holes::None))
}

/// Scala's strings, and an interpolator's (`s"…"`, `f"…"`, `raw"…"`)
/// holding `${…}` and `$name`; and its chars.
pub(super) fn scala(source: &[u8], at: usize) -> Option<Literal> {
    let interpolator =
        match after_word(source, at) || !source.get(at).is_some_and(u8::is_ascii_alphabetic) {
            true => 0,
            false => run(source, at, is_word),
        };
    let from = at + interpolator;
    let holes = match interpolator {
        0 => Holes::None,
        _ => Holes::Dollar { bare: true },
    };
    match source.get(from) {
        Some(b'"') => tripled(source, from, b'"', Escape::None, holes)
            .or_else(|| quoted(source, from, b'"', false, holes)),
        _ if interpolator == 0 => char_literal(source, at),
        _ => None,
    }
}

/// Swift's strings, `"…"` and `"""…"""`, raw with as many `#` around
/// them as they need, holding `\(…)`.
pub(super) fn swift(source: &[u8], at: usize) -> Option<Literal> {
    let hashes = run(source, at, |byte| byte == b'#');
    let from = at + hashes;
    if source.get(from) != Some(&b'"') {
        return None;
    }
    let width = width(source, from, b'"');
    let mut close = vec![b'"'; width];
    close.extend(std::iter::repeat_n(b'#', hashes));
    let escape = match hashes {
        0 => Escape::Backslash,
        _ => Escape::None,
    };
    Some(Literal::new(
        from + width,
        &close,
        escape,
        width == 3,
        Holes::Backslash { hashes },
    ))
}

/// C#'s strings: verbatim `@"…"`, which writes a quote as `""`;
/// interpolated `$"…"`, holding `{…}`; raw `"""…"""`, opened by as many
/// `$` as braces open a hole; and its chars.
pub(super) fn csharp(source: &[u8], at: usize) -> Option<Literal> {
    let sigils = run(source, at, |byte| matches!(byte, b'$' | b'@'));
    let from = at + sigils;
    let dollars = bytes_from(source, at)
        .iter()
        .take(sigils)
        .filter(|&&byte| byte == b'$')
        .count();
    let verbatim = dollars < sigils;
    let holes = match dollars {
        0 => Holes::None,
        open => Holes::Braces { open },
    };
    match run(source, from, |byte| byte == b'"') {
        0 if sigils == 0 => char_literal(source, at),
        0 => None,
        quotes @ 3.. => Some(Literal::new(
            from + quotes,
            &vec![b'"'; quotes],
            Escape::None,
            true,
            holes,
        )),
        _ => {
            let escape = match verbatim {
                true => Escape::Doubled,
                false => Escape::Backslash,
            };
            Some(Literal::new(from + 1, b"\"", escape, verbatim, holes))
        }
    }
}

/// Dart's strings, `'…'`, `"…"` and their tripled forms, holding `${…}`
/// and `$name`, unless raw: `r'…'` holds none.
pub(super) fn dart(source: &[u8], at: usize) -> Option<Literal> {
    let raw = source.get(at) == Some(&b'r') && !after_word(source, at);
    let from = at + usize::from(raw);
    let quote = *source
        .get(from)
        .filter(|&&byte| byte == b'"' || byte == b'\'')?;
    let holes = match raw {
        true => Holes::None,
        false => Holes::Dollar { bare: true },
    };
    let width = width(source, from, quote);
    let escape = match raw {
        true => Escape::None,
        false => Escape::Backslash,
    };
    Some(Literal::new(
        from + width,
        &vec![quote; width],
        escape,
        width == 3,
        holes,
    ))
}

/// Python's strings, with their prefixes: an `f` one holds `{…}`.
pub(super) fn python(source: &[u8], at: usize) -> Option<Literal> {
    let prefix = match after_word(source, at) {
        true => 0,
        false => run(source, at, |byte| b"rRbBuUfF".contains(&byte)).min(2),
    };
    let from = at + prefix;
    let quote = *source
        .get(from)
        .filter(|&&byte| byte == b'"' || byte == b'\'')?;
    let holes = match bytes_from(source, at)
        .iter()
        .take(prefix)
        .any(|byte| byte.eq_ignore_ascii_case(&b'f'))
    {
        true => Holes::Braces { open: 1 },
        false => Holes::None,
    };
    let width = width(source, from, quote);
    Some(Literal::new(
        from + width,
        &vec![quote; width],
        Escape::Backslash,
        width == 3,
        holes,
    ))
}

/// C++'s raw strings, `R"delim(…)delim"`, with their encoding prefixes.
pub(super) fn raw_cpp(source: &[u8], at: usize) -> Option<Literal> {
    let prefix = [&b"u8R\""[..], b"uR\"", b"UR\"", b"LR\"", b"R\""]
        .into_iter()
        .find(|prefix| bytes_from(source, at).starts_with(prefix))
        .filter(|_| !after_word(source, at))?;
    let open = at + prefix.len();
    let delimiter = run(source, open, |byte| {
        !matches!(byte, b'(' | b'"' | b'\\' | b' ' | b'\n')
    });
    if source.get(open + delimiter) != Some(&b'(') {
        return None;
    }
    let mut close = vec![b')'];
    close.extend_from_slice(source.get(open..open + delimiter).unwrap_or_default());
    close.push(b'"');
    Some(Literal::new(
        open + delimiter + 1,
        &close,
        Escape::None,
        true,
        Holes::None,
    ))
}

/// The words after which a script writes a value, so a `/` after one opens
/// a regular expression.
const OPERAND_WORDS: &str = "return typeof case do else in of new delete void throw yield await";

/// A script's regular expression at `at`: a `/` where an operand starts,
/// through the `/` that closes it outside a class `[…]`, and its flags. A
/// `/` after an operand divides, and one that no `/` closes on its line
/// opens nothing.
pub(super) fn regex(source: &[u8], at: usize) -> Option<Literal> {
    if !operand_starts(source, at) {
        return None;
    }
    let mut index = at + 1;
    let mut class = false;
    loop {
        match *source.get(index)? {
            b'\n' => return None,
            b'\\' if source.get(index + 1) != Some(&b'\n') => index += 1,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => break,
            _ => {}
        }
        index += 1;
    }
    let end = index + 1 + run(source, index + 1, |byte| byte.is_ascii_alphabetic());
    Some(Literal::ending(at + 1, end, Holes::None))
}

/// Whether an operand can start at `at` in a script: the last code before
/// it, past blanks and the comments of earlier lines, is a sign that leaves
/// one to come, a word after which a value comes, or nothing at all. A `/`
/// right after a `<` closes a closing tag, and a `/` right before a `>`, with
/// a `}` as the last code before it, closes a self-closing tag; a `/` after
/// `++` or `--` divides. None of these opens one.
fn operand_starts(source: &[u8], at: usize) -> bool {
    if source.get(..at).unwrap_or_default().last() == Some(&b'<') {
        return false;
    }
    let Some(last) = last_code(source, at) else {
        return true;
    };
    let word = source.get(..=last).unwrap_or_default();
    if word.ends_with(b"++") || word.ends_with(b"--") {
        return false;
    }
    match word.last().copied() {
        Some(b'}') => source.get(at + 1) != Some(&b'>'),
        Some(
            b'(' | b',' | b'=' | b':' | b'[' | b'!' | b'&' | b'|' | b'?' | b'{' | b';' | b'+'
            | b'-' | b'*' | b'%' | b'<' | b'>' | b'~' | b'^',
        ) => true,
        Some(byte) if is_word(byte) => {
            let start = word
                .iter()
                .rposition(|&byte| !is_word(byte))
                .map_or(0, |found| found + 1);
            let said = word.get(start..).unwrap_or_default();
            OPERAND_WORDS
                .split_ascii_whitespace()
                .any(|operand| operand.as_bytes() == said)
        }
        _ => false,
    }
}

/// The index of the last byte of code before `at`, past the blanks before it,
/// or `None` where no code stands before it. On `at`'s own line the last
/// non-blank byte is code, since no line comment can stand before `at` there;
/// a block comment that ends there is not read past, so a `/` after one
/// divides. Each earlier line is read from its start by [`last_code_in`].
fn last_code(source: &[u8], at: usize) -> Option<usize> {
    let before = source.get(..at).unwrap_or_default();
    let last = before
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())?;
    if !before.get(last..).unwrap_or_default().contains(&b'\n') {
        return Some(last);
    }
    let mut end = at;
    for line in before.rsplit(|&byte| byte == b'\n') {
        let start = end - line.len();
        if let Some(found) = last_code_in(line) {
            return Some(start + found);
        }
        end = start.saturating_sub(1);
    }
    None
}

/// The index in `line`, a script's line read from its start, of the last byte
/// of code that is not blank, or `None` where the line holds none. A comment
/// is passed over, and a quoted text counts as code up to its close, or to
/// the line's end where it has none. A `\` keeps the byte after it from
/// opening a comment or a quote, as the `\/` of a regular expression does.
fn last_code_in(line: &[u8]) -> Option<usize> {
    let mut found = None;
    let mut index = 0;
    while let Some(&byte) = line.get(index) {
        if let Some(end) = comment(Language::Script, line, index) {
            index = end;
        } else if matches!(byte, b'"' | b'\'' | b'`') {
            index = quote_end(line, index, byte);
            found = Some(index - 1);
        } else if byte == b'\\' {
            found = Some(index);
            index += 2;
        } else {
            if !byte.is_ascii_whitespace() {
                found = Some(index);
            }
            index += 1;
        }
    }
    found
}

/// Just past the `quote` that closes the text it opens at `at` in `line`, or
/// the line's end where none closes on it: a `\` keeps the byte after it from
/// closing.
fn quote_end(line: &[u8], at: usize, quote: u8) -> usize {
    let mut index = at + 1;
    while let Some(&byte) = line.get(index) {
        if byte == quote {
            return index + 1;
        }
        index += if byte == b'\\' { 2 } else { 1 };
    }
    line.len()
}

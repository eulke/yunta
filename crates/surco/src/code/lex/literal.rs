//! How each language writes what is not code: its comments, and the
//! literals whose text is not code, with the holes in that text that are.
//! The literals of the programming languages that go past a plain quote, a
//! script's regular expressions among them, are read in [`languages`]; shell's,
//! YAML's, TOML's, JSON's and Markdown's in [`data`].

mod data;
mod languages;
pub(crate) use data::Fence;

use super::Language;
use languages::{csharp, dart, python, raw_cpp, regex, rust, scala, swift};

/// A literal whose text is not code: where its text starts, what ends
/// it, and how code is written into it.
pub(super) struct Literal {
    pub(super) text: usize,
    pub(super) close: Close,
    /// How the close is written into the text without closing it.
    pub(super) escape: Escape,
    /// Whether its text runs on past a line break; one that does not ends
    /// at the break, and what follows it is code.
    pub(super) lines: bool,
    pub(super) holes: Holes,
}

/// What ends a literal.
pub(super) enum Close {
    /// These bytes, where they first stand past the text's start.
    Bytes(Vec<u8>),
    /// The byte its reader found it ends at: where a fence's closing line, a
    /// here-document's word or a block scalar's last line ends.
    At(usize),
}

/// How a literal writes its close into its text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Escape {
    /// It does not: the close always closes.
    None,
    /// `\` keeps the byte after it from closing the literal.
    Backslash,
    /// The close written twice is the close's bytes in the text, as YAML's
    /// `''` and C#'s verbatim `""` are.
    Doubled,
}

/// How a literal's text holds code.
#[derive(Clone, Copy)]
pub(super) enum Holes {
    None,
    /// `${…}`, and `$name` too when `bare`.
    Dollar {
        bare: bool,
    },
    /// `{…}`, opened by `open` braces in a row: with one, `{{` is a brace
    /// of the text; with more, fewer than `open` are.
    Braces {
        open: usize,
    },
    /// `\(…)`, with as many `#` after the `\` as the literal opened with.
    Backslash {
        hashes: usize,
    },
    /// `${…}`, `$(…)` and `$name`, as a shell's double quotes hold them.
    Shell,
    /// A here-document's: the rest of its operator's line, from `line`,
    /// which is code; and, where it `expands`, a shell's `${…}`, `$(…)` and
    /// `$name`.
    Heredoc {
        line: usize,
        expands: bool,
    },
}

/// Code in a literal's text, found where it starts.
pub(super) enum Hole {
    /// Code from `from` up to the `close` that ends it.
    Code { from: usize, close: u8 },
    /// A name, from `from` to `to`.
    Name { from: usize, to: usize },
    /// Text that looks like a hole and is not, up to `to`.
    Text { to: usize },
}

/// Where the comment that starts at `at` ends, if one does.
pub(super) fn comment(language: Language, source: &[u8], at: usize) -> Option<usize> {
    let rest = bytes_from(source, at);
    match language {
        Language::Python | Language::Toml => rest.starts_with(b"#").then(|| line_end(source, at)),
        Language::Shell | Language::Yaml => data::word_comment(source, at),
        Language::Markdown => data::markdown_comment(source, at),
        Language::Json => None,
        Language::Rust
        | Language::C
        | Language::Java
        | Language::Go
        | Language::Kotlin
        | Language::Scala
        | Language::Swift
        | Language::CSharp
        | Language::Dart
        | Language::Script => {
            if rest.starts_with(b"//") {
                Some(line_end(source, at))
            } else if rest.starts_with(b"/*") {
                Some(block_end(source, at, nests(language)))
            } else {
                None
            }
        }
    }
}

/// Whether a block comment holds others: `/* /* */ */` is one.
fn nests(language: Language) -> bool {
    matches!(
        language,
        Language::Rust | Language::Kotlin | Language::Scala | Language::Swift | Language::Dart
    )
}

/// The literal that starts at `at`, if one does.
pub(super) fn literal(language: Language, source: &[u8], at: usize) -> Option<Literal> {
    match language {
        Language::Rust => rust(source, at),
        Language::C => raw_cpp(source, at).or_else(|| quoted_or_char(source, at)),
        Language::Java => tripled(source, at, b'"', Escape::Backslash, Holes::None)
            .or_else(|| quoted_or_char(source, at)),
        Language::Go => match source.get(at) {
            Some(b'`') => Some(Literal::new(at + 1, b"`", Escape::None, true, Holes::None)),
            _ => quoted_or_char(source, at),
        },
        Language::Kotlin => {
            let holes = Holes::Dollar { bare: true };
            tripled(source, at, b'"', Escape::None, holes)
                .or_else(|| quoted(source, at, b'"', false, holes))
                .or_else(|| char_literal(source, at))
        }
        Language::Scala => scala(source, at),
        Language::Swift => swift(source, at),
        Language::CSharp => csharp(source, at),
        Language::Dart => dart(source, at),
        Language::Script => match source.get(at).copied() {
            Some(b'`') => Some(Literal::new(
                at + 1,
                b"`",
                Escape::Backslash,
                true,
                Holes::Dollar { bare: false },
            )),
            Some(quote @ (b'"' | b'\'')) => quoted(source, at, quote, false, Holes::None),
            Some(b'/') => regex(source, at),
            _ => None,
        },
        Language::Python => python(source, at),
        Language::Shell => data::shell(source, at),
        Language::Yaml => data::yaml(source, at),
        Language::Toml => data::toml(source, at),
        Language::Json => data::json(source, at),
        Language::Markdown => data::markdown(source, at),
    }
}

impl Literal {
    /// A literal whose text starts at `text` and that `close` ends.
    fn new(text: usize, close: &[u8], escape: Escape, lines: bool, holes: Holes) -> Literal {
        Literal {
            text,
            close: Close::Bytes(close.to_vec()),
            escape,
            lines,
            holes,
        }
    }

    /// A literal whose reader found where it ends: its text from `text` up
    /// to `end`, holding code as `holes` say.
    fn ending(text: usize, end: usize, holes: Holes) -> Literal {
        Literal {
            text,
            close: Close::At(end),
            escape: Escape::None,
            lines: true,
            holes,
        }
    }
}

impl Holes {
    /// The hole that starts at `at` in a literal's text, if one does.
    pub(super) fn at(self, source: &[u8], at: usize) -> Option<Hole> {
        let rest = bytes_from(source, at);
        match self {
            Holes::None => None,
            Holes::Dollar { bare } => match rest {
                [b'$', b'{', ..] => Some(Hole::Code {
                    from: at + 2,
                    close: b'}',
                }),
                [b'$', next, ..] if bare && (next.is_ascii_alphabetic() || *next == b'_') => {
                    Some(Hole::Name {
                        from: at + 1,
                        to: at + 1 + run(source, at + 1, is_word),
                    })
                }
                _ => None,
            },
            Holes::Braces { open } => braces_hole(source, at, open),
            Holes::Backslash { hashes } => {
                let opens = rest.first() == Some(&b'\\')
                    && run(source, at + 1, |byte| byte == b'#') == hashes
                    && source.get(at + 1 + hashes) == Some(&b'(');
                opens.then(|| Hole::Code {
                    from: at + hashes + 2,
                    close: b')',
                })
            }
            Holes::Shell => shell_hole(source, at),
            Holes::Heredoc { line, expands } => match at == line && rest.first() != Some(&b'\n') {
                true => Some(Hole::Code {
                    from: at,
                    close: b'\n',
                }),
                false if expands => shell_hole(source, at),
                false => None,
            },
        }
    }
}

/// The hole that starts at `at` in a literal's text, where `open` braces in
/// a row open code: with one, an odd run does, and with more, a run of at
/// least `open` does. A run that does not open code is text.
fn braces_hole(source: &[u8], at: usize, open: usize) -> Option<Hole> {
    let braces = run(source, at, |byte| byte == b'{');
    let code = match open {
        1 => braces % 2 == 1,
        _ => braces >= open,
    };
    match (braces, code) {
        (0, _) => None,
        (_, true) => Some(Hole::Code {
            from: at + braces,
            close: b'}',
        }),
        (_, false) => Some(Hole::Text { to: at + braces }),
    }
}

/// The hole that starts at `at` in a shell's double-quoted text: `${…}` and
/// `$(…)` hold code up to their closing bracket, and `$name` holds a name.
fn shell_hole(source: &[u8], at: usize) -> Option<Hole> {
    match bytes_from(source, at) {
        [b'$', b'{', ..] => Some(Hole::Code {
            from: at + 2,
            close: b'}',
        }),
        [b'$', b'(', ..] => Some(Hole::Code {
            from: at + 2,
            close: b')',
        }),
        [b'$', next, ..] if next.is_ascii_alphabetic() || *next == b'_' => Some(Hole::Name {
            from: at + 1,
            to: at + 1 + run(source, at + 1, is_word),
        }),
        _ => None,
    }
}

/// How many times `quote` opens the literal at `at`: three for a tripled
/// one, one otherwise.
fn width(source: &[u8], at: usize, quote: u8) -> usize {
    match source.get(at..at + 3) == Some(&[quote; 3][..]) {
        true => 3,
        false => 1,
    }
}

/// A literal opened by `quote` three times at `at`.
fn tripled(source: &[u8], at: usize, quote: u8, escape: Escape, holes: Holes) -> Option<Literal> {
    (source.get(at..at + 3) == Some(&[quote; 3][..]))
        .then(|| Literal::new(at + 3, &[quote; 3], escape, true, holes))
}

/// A literal opened by `quote` at `at`, closed by the next one no `\`
/// escapes. `lines` says whether its text runs on past a line break.
fn quoted(source: &[u8], at: usize, quote: u8, lines: bool, holes: Holes) -> Option<Literal> {
    (source.get(at) == Some(&quote))
        .then(|| Literal::new(at + 1, &[quote], Escape::Backslash, lines, holes))
}

/// A string `"…"` or a char at `at`.
fn quoted_or_char(source: &[u8], at: usize) -> Option<Literal> {
    quoted(source, at, b'"', false, Holes::None).or_else(|| char_literal(source, at))
}

/// A char literal at `at`, as `'x'`, `'\n'` or `'\''`, and not a quote that
/// opens none: a lifetime, `'a`.
fn char_literal(source: &[u8], at: usize) -> Option<Literal> {
    let body = source
        .get(at + 1..)
        .filter(|_| source.get(at) == Some(&b'\''))?;
    let close = match body.first()? {
        b'\\' => body.iter().skip(2).position(|&b| b == b'\'')? + 2,
        _ => body.iter().take(5).position(|&b| b == b'\'')?,
    };
    (close <= 10).then(|| Literal::new(at + 1, b"'", Escape::Backslash, false, Holes::None))
}

/// Just past the block comment that opens at `at`: at its first `*/`, or,
/// where comments nest, at the one that closes every comment opened.
fn block_end(source: &[u8], at: usize, nests: bool) -> usize {
    let mut depth = 0usize;
    let mut index = at;
    while index < source.len() {
        let rest = bytes_from(source, index);
        if rest.starts_with(b"/*") && (nests || depth == 0) {
            depth += 1;
            index += 2;
        } else if rest.starts_with(b"*/") {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return index;
            }
        } else {
            index += 1;
        }
    }
    source.len()
}

/// Where the line holding `at` ends, its newline left in place.
fn line_end(source: &[u8], at: usize) -> usize {
    source
        .iter()
        .skip(at)
        .position(|&b| b == b'\n')
        .map_or(source.len(), |found| at + found)
}

/// `source` from `at` on, nothing past its end.
pub(super) fn bytes_from(source: &[u8], at: usize) -> &[u8] {
    source.get(at..).unwrap_or_default()
}

/// How many bytes from `at` on `keep` holds for.
fn run(source: &[u8], at: usize, keep: impl Fn(u8) -> bool) -> usize {
    source
        .iter()
        .skip(at)
        .take_while(|&&byte| keep(byte))
        .count()
}

/// Whether the byte before `at` belongs to a name.
fn after_word(source: &[u8], at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|before| source.get(before))
        .is_some_and(|&byte| is_word(byte))
}

/// Whether `byte` belongs to a name: a letter, a digit or `_`.
pub(in crate::code) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

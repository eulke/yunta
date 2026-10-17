//! What hides code in each language the reading knows: its comments, and
//! the literals whose text is not code — with the holes in that text
//! that are.

/// A family of languages the reading knows, by file extension.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Lang {
    Rust,
    C,
    Java,
    Go,
    Kotlin,
    Scala,
    Swift,
    CSharp,
    Dart,
    Js,
    Python,
}

/// A literal whose text is not code: where its text starts, what ends
/// it, and how code is written into it.
pub(super) struct Literal {
    pub(super) text: usize,
    pub(super) close: Vec<u8>,
    /// Whether `\` keeps the byte after it from closing the literal.
    pub(super) escapes: bool,
    pub(super) holes: Holes,
}

/// How a literal's text holds code.
#[derive(Clone, Copy)]
pub(super) enum Holes {
    None,
    /// `${…}` — and, when `bare`, `$name`.
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

impl Lang {
    pub(super) fn of(kind: &str) -> Option<Lang> {
        Some(match kind {
            "rs" => Lang::Rust,
            "c" | "h" | "cc" | "cpp" | "hpp" => Lang::C,
            "java" => Lang::Java,
            "go" => Lang::Go,
            "kt" | "kts" => Lang::Kotlin,
            "scala" => Lang::Scala,
            "swift" => Lang::Swift,
            "cs" => Lang::CSharp,
            "dart" => Lang::Dart,
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::Js,
            "py" => Lang::Python,
            _ => return None,
        })
    }

    /// Where the comment that starts at `at` ends, if one does.
    pub(super) fn comment(self, source: &[u8], at: usize) -> Option<usize> {
        let rest = bytes_from(source, at);
        match self {
            Lang::Python => rest.starts_with(b"#").then(|| line_end(source, at)),
            _ if rest.starts_with(b"//") => Some(line_end(source, at)),
            _ if rest.starts_with(b"/*") => Some(block_end(source, at, self.nests())),
            _ => None,
        }
    }

    /// Whether a block comment holds others: `/* /* */ */` is one.
    fn nests(self) -> bool {
        matches!(
            self,
            Lang::Rust | Lang::Kotlin | Lang::Scala | Lang::Swift | Lang::Dart
        )
    }

    /// The literal that starts at `at`, if one does.
    pub(super) fn literal(self, source: &[u8], at: usize) -> Option<Literal> {
        match self {
            Lang::Rust => rust(source, at),
            Lang::C => raw_cpp(source, at).or_else(|| quoted_or_char(source, at)),
            Lang::Java => {
                tripled(source, at, b'"', true, Holes::None).or_else(|| quoted_or_char(source, at))
            }
            Lang::Go => match source.get(at) {
                Some(b'`') => Some(Literal::new(at + 1, b"`", false, Holes::None)),
                _ => quoted_or_char(source, at),
            },
            Lang::Kotlin => {
                let holes = Holes::Dollar { bare: true };
                tripled(source, at, b'"', false, holes)
                    .or_else(|| quoted(source, at, b'"', holes))
                    .or_else(|| char_literal(source, at))
            }
            Lang::Scala => scala(source, at),
            Lang::Swift => swift(source, at),
            Lang::CSharp => csharp(source, at),
            Lang::Dart => dart(source, at),
            Lang::Js => match source.get(at).copied() {
                Some(b'`') => Some(Literal::new(
                    at + 1,
                    b"`",
                    true,
                    Holes::Dollar { bare: false },
                )),
                Some(quote @ (b'"' | b'\'')) => quoted(source, at, quote, Holes::None),
                _ => None,
            },
            Lang::Python => python(source, at),
        }
    }
}

impl Literal {
    fn new(text: usize, close: &[u8], escapes: bool, holes: Holes) -> Literal {
        Literal {
            text,
            close: close.to_vec(),
            escapes,
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
            Holes::Braces { open } => {
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
            Holes::Backslash { hashes } => {
                let opens = rest.first() == Some(&b'\\')
                    && run(source, at + 1, |byte| byte == b'#') == hashes
                    && source.get(at + 1 + hashes) == Some(&b'(');
                opens.then(|| Hole::Code {
                    from: at + hashes + 2,
                    close: b')',
                })
            }
        }
    }
}

/// Rust's strings — `"…"`, raw `r#"…"#`, byte and C ones — and chars.
fn rust(source: &[u8], at: usize) -> Option<Literal> {
    let starts = !after_word(source, at);
    let prefixed = starts && matches!(source.get(at), Some(b'b' | b'c'));
    let from = at + usize::from(prefixed);
    if starts && source.get(from) == Some(&b'r') {
        let hashes = run(source, from + 1, |byte| byte == b'#');
        let open = from + 1 + hashes;
        if source.get(open) == Some(&b'"') {
            let mut close = vec![b'"'];
            close.extend(std::iter::repeat_n(b'#', hashes));
            return Some(Literal::new(open + 1, &close, false, Holes::None));
        }
    }
    match prefixed {
        true => quoted_or_char(source, from),
        false => quoted_or_char(source, at),
    }
}

/// Scala's strings, an interpolator's — `s"…"`, `f"…"`, `raw"…"` —
/// holding `${…}` and `$name`; and chars.
fn scala(source: &[u8], at: usize) -> Option<Literal> {
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
        Some(b'"') => {
            tripled(source, from, b'"', false, holes).or_else(|| quoted(source, from, b'"', holes))
        }
        _ if interpolator == 0 => char_literal(source, at),
        _ => None,
    }
}

/// Swift's strings, `"…"` and `"""…"""`, raw with as many `#` around
/// them as they need, holding `\(…)`.
fn swift(source: &[u8], at: usize) -> Option<Literal> {
    let hashes = run(source, at, |byte| byte == b'#');
    let from = at + hashes;
    if source.get(from) != Some(&b'"') {
        return None;
    }
    let width = width(source, from, b'"');
    let mut close = vec![b'"'; width];
    close.extend(std::iter::repeat_n(b'#', hashes));
    Some(Literal::new(
        from + width,
        &close,
        hashes == 0,
        Holes::Backslash { hashes },
    ))
}

/// C#'s strings: verbatim `@"…"`, interpolated `$"…"` holding `{…}`,
/// raw `"""…"""` opened by as many `$` as braces open a hole; and chars.
fn csharp(source: &[u8], at: usize) -> Option<Literal> {
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
            false,
            holes,
        )),
        _ => Some(Literal::new(from + 1, b"\"", !verbatim, holes)),
    }
}

/// Dart's strings, `'…'`, `"…"` and their tripled forms, holding `${…}`
/// and `$name` — but for a raw one, `r'…'`.
fn dart(source: &[u8], at: usize) -> Option<Literal> {
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
    Some(Literal::new(from + width, &vec![quote; width], !raw, holes))
}

/// Python's strings, with their prefixes: an `f` one holds `{…}`.
fn python(source: &[u8], at: usize) -> Option<Literal> {
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
    Some(Literal::new(from + width, &vec![quote; width], true, holes))
}

/// C++'s raw strings, `R"delim(…)delim"`, with their encoding prefixes.
fn raw_cpp(source: &[u8], at: usize) -> Option<Literal> {
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
        false,
        Holes::None,
    ))
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
fn tripled(source: &[u8], at: usize, quote: u8, escapes: bool, holes: Holes) -> Option<Literal> {
    (source.get(at..at + 3) == Some(&[quote; 3][..]))
        .then(|| Literal::new(at + 3, &[quote; 3], escapes, holes))
}

/// A literal opened by `quote` at `at`, closed by the next unescaped one.
fn quoted(source: &[u8], at: usize, quote: u8, holes: Holes) -> Option<Literal> {
    (source.get(at) == Some(&quote)).then(|| Literal::new(at + 1, &[quote], true, holes))
}

/// A string `"…"` or a char at `at`.
fn quoted_or_char(source: &[u8], at: usize) -> Option<Literal> {
    quoted(source, at, b'"', Holes::None).or_else(|| char_literal(source, at))
}

/// A char literal at `at` — `'x'`, `'\n'`, `'\''` — and not a quote that
/// opens none: a lifetime, `'a`.
fn char_literal(source: &[u8], at: usize) -> Option<Literal> {
    let body = source
        .get(at + 1..)
        .filter(|_| source.get(at) == Some(&b'\''))?;
    let close = match body.first()? {
        b'\\' => body.iter().skip(2).position(|&b| b == b'\'')? + 2,
        _ => body.iter().take(5).position(|&b| b == b'\'')?,
    };
    (close <= 10).then(|| Literal::new(at + 1, b"'", true, Holes::None))
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

/// `source` from `at` on — nothing past its end.
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

pub(super) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

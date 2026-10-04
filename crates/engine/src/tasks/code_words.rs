//! Whether a file names a word as code: outside its string literals and
//! its comments. A test fixture that quotes a plan, or a doc comment that
//! mentions a shape, says the name and calls nothing.
//!
//! The reading is a lexer of the few things that hide code, per family of
//! languages — strings, raw strings, template literals, char literals,
//! comments — and nothing more. A file of a type it does not know is read
//! whole, so an unknown language never loses a real caller.

/// Whether `source`, a file whose extension is `kind`, names `word` as
/// code.
pub(crate) fn names_in_code(source: &str, kind: &str, word: &str) -> bool {
    let code = match Syntax::of(kind) {
        Some(syntax) => blanked(source.as_bytes(), syntax),
        None => source.as_bytes().to_vec(),
    };
    contains_word(&code, word.as_bytes())
}

/// What hides code in a family of languages.
#[derive(Clone, Copy)]
struct Syntax {
    /// `//` and `/* */`, rather than `#`.
    slashes: bool,
    /// Rust's `r"…"` and `r#"…"#`.
    raw_strings: bool,
    /// JavaScript's `` `…` ``.
    templates: bool,
    /// Python's `'''…'''` and `"""…"""`, and `'…'` as a string.
    triple_quotes: bool,
}

impl Syntax {
    fn of(kind: &str) -> Option<Syntax> {
        let c_like = Syntax {
            slashes: true,
            raw_strings: false,
            templates: false,
            triple_quotes: false,
        };
        match kind {
            "rs" => Some(Syntax {
                raw_strings: true,
                ..c_like
            }),
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Some(Syntax {
                templates: true,
                ..c_like
            }),
            "c" | "h" | "cc" | "cpp" | "hpp" | "go" | "java" | "kt" | "kts" | "swift" | "cs"
            | "scala" | "dart" => Some(c_like),
            "py" => Some(Syntax {
                slashes: false,
                triple_quotes: true,
                ..c_like
            }),
            _ => None,
        }
    }
}

/// `source` with every string and comment turned to spaces — newlines
/// kept — so what is left is the code, word boundaries intact.
fn blanked(source: &[u8], syntax: Syntax) -> Vec<u8> {
    let mut out = source.to_vec();
    let mut at = 0;
    while at < source.len() {
        let hidden = hidden_from(source, at, syntax);
        let end = hidden.unwrap_or(at + 1);
        if hidden.is_some() {
            for byte in out.iter_mut().take(end).skip(at) {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        at = end;
    }
    out
}

/// Where what starts at `at` ends, when it is a string or a comment.
fn hidden_from(source: &[u8], at: usize, syntax: Syntax) -> Option<usize> {
    let rest = source.get(at..)?;
    let after_word = at
        .checked_sub(1)
        .and_then(|before| source.get(before))
        .is_some_and(|&byte| is_word(byte));
    let first = *rest.first()?;
    let tripled = rest.get(1..3) == Some(&[first, first][..]);
    match first {
        b'/' if syntax.slashes && rest.starts_with(b"//") => Some(line_end(source, at)),
        b'/' if syntax.slashes && rest.starts_with(b"/*") => Some(past(source, at + 2, b"*/")),
        b'#' if !syntax.slashes => Some(line_end(source, at)),
        b'r' if syntax.raw_strings && !after_word => raw_string(source, at),
        b'`' if syntax.templates => Some(quoted(source, at, b'`')),
        b'"' | b'\'' if syntax.triple_quotes && tripled => Some(past(source, at + 3, &[first; 3])),
        b'"' => Some(quoted(source, at, b'"')),
        b'\'' if syntax.triple_quotes => Some(quoted(source, at, b'\'')),
        b'\'' => char_literal(source, at),
        _ => None,
    }
}

/// The end of a quoted run that opens at `at` with `quote`, past its
/// closing quote — an escaped quote does not close it.
fn quoted(source: &[u8], at: usize, quote: u8) -> usize {
    let mut index = at + 1;
    while let Some(&byte) = source.get(index) {
        match byte {
            b'\\' => index += 2,
            _ if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    source.len()
}

/// The end of a char literal at `at` — `'x'`, `'\n'`, `'\''` — or `None`
/// for a quote that opens none: a lifetime, `'a`.
fn char_literal(source: &[u8], at: usize) -> Option<usize> {
    let body = source.get(at + 1..)?;
    let close = match body.first()? {
        b'\\' => body.iter().skip(2).position(|&b| b == b'\'')? + 2,
        _ => body.iter().take(5).position(|&b| b == b'\'')?,
    };
    (close <= 10).then_some(at + 1 + close + 1)
}

/// The end of a Rust raw string at `at` — `r"…"`, `r#"…"#` — or `None`
/// when the `r` starts a name instead.
fn raw_string(source: &[u8], at: usize) -> Option<usize> {
    let hashes = source
        .iter()
        .skip(at + 1)
        .take_while(|&&b| b == b'#')
        .count();
    let open = at + 1 + hashes;
    if source.get(open) != Some(&b'"') {
        return None;
    }
    let mut close = vec![b'"'];
    close.extend(std::iter::repeat_n(b'#', hashes));
    Some(past(source, open + 1, &close))
}

/// Just past the first `end` at or after `from`, or the source's end.
fn past(source: &[u8], from: usize, end: &[u8]) -> usize {
    source
        .get(from..)
        .and_then(|rest| rest.windows(end.len()).position(|window| window == end))
        .map_or(source.len(), |found| from + found + end.len())
}

/// Where the line holding `at` ends, its newline left in place.
fn line_end(source: &[u8], at: usize) -> usize {
    source
        .iter()
        .skip(at)
        .position(|&b| b == b'\n')
        .map_or(source.len(), |found| at + found)
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `word` stands whole somewhere in `code`.
fn contains_word(code: &[u8], word: &[u8]) -> bool {
    if word.is_empty() {
        return false;
    }
    code.windows(word.len())
        .enumerate()
        .filter(|(_, window)| *window == word)
        .any(|(at, _)| {
            let before = at.checked_sub(1).and_then(|i| code.get(i)).copied();
            let after = code.get(at + word.len()).copied();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
}

#[cfg(test)]
mod tests {
    use super::names_in_code;

    #[test]
    fn a_name_in_a_fixture_string_calls_nothing() {
        let fixture = "let plan = r##\"\n  - name: PackStore\n    code: PackStore::new()\n\"##;\n";
        assert!(!names_in_code(fixture, "rs", "PackStore"));
        let quoted = "let at = \"src/pack.rs::PackStore\"; // PackStore\n";
        assert!(!names_in_code(quoted, "rs", "PackStore"));
        assert!(!names_in_code(
            "/* PackStore */ fn f() {}",
            "rs",
            "PackStore"
        ));
    }

    #[test]
    fn a_name_in_code_is_a_caller() {
        assert!(names_in_code(
            "let store = PackStore::new();",
            "rs",
            "PackStore"
        ));
        let lifetime = "fn f<'a>(s: &'a PackStore) -> char { 'x' }";
        assert!(names_in_code(lifetime, "rs", "PackStore"));
        assert!(names_in_code(
            "const s = `${a}`; new PackStore()",
            "ts",
            "PackStore"
        ));
    }

    #[test]
    fn python_hides_its_own_strings_and_comments() {
        assert!(!names_in_code(
            "# PackStore\nx = '''PackStore'''\n",
            "py",
            "PackStore"
        ));
        assert!(names_in_code(
            "x = PackStore()  # the store\n",
            "py",
            "PackStore"
        ));
    }

    #[test]
    fn a_language_it_does_not_know_is_read_whole() {
        assert!(names_in_code("store: \"PackStore\"\n", "yaml", "PackStore"));
        assert!(!names_in_code("PackStores", "yaml", "PackStore"));
    }
}

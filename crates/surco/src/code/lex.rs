//! Code as its language reads it: what of it is a string, a character or
//! a comment and what is code, and the words a reader compares it in.

use std::ops::Range;
use std::path::Path;

/// A language this reads, by how its source says what is code and what is
/// a string or a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
    Go,
    Script,
}

impl Language {
    /// The language of the file at `path`, by its extension; `None` for a
    /// file in a language this does not read.
    pub(crate) fn of(path: &str) -> Option<Self> {
        match Path::new(path).extension().and_then(|ext| ext.to_str()) {
            Some("rs") => Some(Language::Rust),
            Some("go") => Some(Language::Go),
            Some("js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx") => Some(Language::Script),
            _ => None,
        }
    }
}

/// Where the lexer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lexed {
    Code,
    /// Inside a block comment, nested this deep.
    Comment(usize),
    /// Inside a string closed by this quote.
    Quoted(char),
    /// Inside a Rust raw string closed by a quote and this many hashes.
    Raw(usize),
}

/// One step through `bytes` from `at`, standing in `state`: where the lexer
/// stands after it, and where the next step starts. A comment that runs to
/// the end of its line is read in one step, up to the line's break.
pub(crate) fn step(bytes: &[u8], at: usize, state: Lexed, language: Language) -> (Lexed, usize) {
    match state {
        Lexed::Comment(depth) => commented(bytes, at, depth, language),
        Lexed::Quoted(quote) => quoted(bytes, at, quote, language),
        Lexed::Raw(hashes) => raw(bytes, at, hashes),
        Lexed::Code => code(bytes, at, language),
    }
}

/// Where the lexer stands once a line ends in `state`: a string a line
/// leaves open closes with it, except the ones a language lets run on.
pub(crate) fn line_ended(state: Lexed) -> Lexed {
    match state {
        Lexed::Quoted(quote) if quote != '`' && quote != '"' => Lexed::Code,
        state => state,
    }
}

/// One step through a block comment `depth` deep.
fn commented(bytes: &[u8], at: usize, depth: usize, language: Language) -> (Lexed, usize) {
    match (bytes[at], bytes.get(at + 1)) {
        (b'*', Some(b'/')) if depth == 1 => (Lexed::Code, at + 2),
        (b'*', Some(b'/')) => (Lexed::Comment(depth - 1), at + 2),
        (b'/', Some(b'*')) if language == Language::Rust => (Lexed::Comment(depth + 1), at + 2),
        _ => (Lexed::Comment(depth), at + 1),
    }
}

/// One step through a string closed by `quote`. A backslash escapes what
/// follows, except in Go's raw strings.
fn quoted(bytes: &[u8], at: usize, quote: char, language: Language) -> (Lexed, usize) {
    match bytes[at] {
        b'\\' if !(language == Language::Go && quote == '`') => (Lexed::Quoted(quote), at + 2),
        byte if byte == quote as u8 => (Lexed::Code, at + 1),
        _ => (Lexed::Quoted(quote), at + 1),
    }
}

/// One step through a Rust raw string closed by a quote and `hashes`.
fn raw(bytes: &[u8], at: usize, hashes: usize) -> (Lexed, usize) {
    let closes = bytes[at] == b'"'
        && bytes.len() > at + hashes
        && bytes[at + 1..=at + hashes].iter().all(|byte| *byte == b'#');
    match closes {
        true => (Lexed::Code, at + 1 + hashes),
        false => (Lexed::Raw(hashes), at + 1),
    }
}

/// One step through code: a comment to the end of its line, what opens a
/// block comment, a string or a character literal, or one byte of code.
fn code(bytes: &[u8], at: usize, language: Language) -> (Lexed, usize) {
    match (bytes[at], bytes.get(at + 1).copied()) {
        (b'/', Some(b'/')) => {
            let ends = bytes[at..].iter().position(|byte| *byte == b'\n');
            (Lexed::Code, ends.map_or(bytes.len(), |ends| at + ends))
        }
        (b'/', Some(b'*')) => (Lexed::Comment(1), at + 2),
        (b'"', _) => (Lexed::Quoted('"'), at + 1),
        (b'`', _) if language != Language::Rust => (Lexed::Quoted('`'), at + 1),
        (b'\'', _) if language == Language::Script => (Lexed::Quoted('\''), at + 1),
        // A character literal, never a lifetime: `'{'`, `'\n'`.
        (b'\'', Some(b'\\')) => (Lexed::Quoted('\''), at + 1),
        (b'\'', _) if bytes.get(at + 2) == Some(&b'\'') => (Lexed::Code, at + 3),
        (b'r', Some(b'"' | b'#')) if language == Language::Rust && raw_opens(bytes, at) => {
            let hashes = bytes[at + 1..]
                .iter()
                .take_while(|byte| **byte == b'#')
                .count();
            (Lexed::Raw(hashes), at + 2 + hashes)
        }
        _ => (Lexed::Code, at + 1),
    }
}

/// Whether the `r` at `at` opens a raw string rather than ending a name.
fn raw_opens(bytes: &[u8], at: usize) -> bool {
    let named = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    let hashes = bytes[at + 1..].iter().take_while(|b| **b == b'#').count();
    !named && bytes.get(at + 1 + hashes) == Some(&b'"')
}

/// The words of `text`, code in `language`, by where each stands: a run of
/// letters, digits and underscores, a whole string, character or raw
/// literal, or any other single sign. Whitespace, comments and commas are
/// not words: a comma separates, and where one stands is how a formatter
/// writes it. Code in a language this does not read has no strings or
/// comments.
pub(crate) fn words(text: &str, language: Option<Language>) -> Vec<Range<usize>> {
    let mut words = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let (stretch, end) = stretch(text, at, language);
        if stretch == Stretch::Word {
            words.push(at..end);
        }
        at = end;
    }
    words
}

/// `gap`, what stands between two words of code in `language`, without its
/// comments: the whitespace and the commas a reader sees between them.
pub(crate) fn uncommented(gap: &str, language: Option<Language>) -> String {
    let mut kept = String::new();
    let mut at = 0;
    while at < gap.len() {
        let (stretch, end) = stretch(gap, at, language);
        if stretch != Stretch::Comment {
            kept.push_str(&gap[at..end]);
        }
        at = end;
    }
    kept
}

/// What a stretch of code is to a reader of its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stretch {
    Word,
    Comment,
    /// A whitespace character or a comma.
    Blank,
}

/// The stretch of `text` that starts at `at`, and where it ends.
fn stretch(text: &str, at: usize, language: Option<Language>) -> (Stretch, usize) {
    let Some(first) = text[at..].chars().next() else {
        return (Stretch::Blank, text.len());
    };
    if first.is_whitespace() || first == ',' {
        return (Stretch::Blank, at + first.len_utf8());
    }
    if let Some(enclosed) = language.and_then(|language| enclosed(text.as_bytes(), at, language)) {
        return enclosed;
    }
    let named = |ch: char| ch.is_alphanumeric() || ch == '_';
    let length = match named(first) {
        true => text[at..]
            .find(|ch: char| !named(ch))
            .unwrap_or(text.len() - at),
        false => first.len_utf8(),
    };
    (Stretch::Word, at + length)
}

/// The literal or the comment that opens at `at` of `bytes`, and where it
/// ends; `None` where neither opens there.
fn enclosed(bytes: &[u8], at: usize, language: Language) -> Option<(Stretch, usize)> {
    let (mut state, mut end) = step(bytes, at, Lexed::Code, language);
    let stretch = match state {
        // A character literal read whole, or a comment to its line's end.
        Lexed::Code if end > at + 1 => match bytes[at] {
            b'\'' => Stretch::Word,
            _ => Stretch::Comment,
        },
        Lexed::Code => return None,
        Lexed::Comment(_) => Stretch::Comment,
        Lexed::Quoted(_) | Lexed::Raw(_) => Stretch::Word,
    };
    while state != Lexed::Code && end < bytes.len() {
        (state, end) = match (bytes[end], line_ended(state)) {
            // A string its line leaves open closes before the break.
            (b'\n', Lexed::Code) => (Lexed::Code, end),
            _ => step(bytes, end, state, language),
        };
    }
    Some((stretch, end.min(bytes.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words of `text` as Rust, each as it reads.
    fn read(text: &str) -> Vec<&str> {
        words(text, Some(Language::Rust))
            .into_iter()
            .map(|word| &text[word])
            .collect()
    }

    #[test]
    fn a_word_is_a_name_a_literal_or_a_sign_and_never_a_comment_or_a_comma() {
        assert_eq!(
            read("pub fn f(a: u8, /* b, */ s: &str) -> char; // c, d\n'{'"),
            [
                "pub", "fn", "f", "(", "a", ":", "u8", "s", ":", "&", "str", ")", "-", ">", "char",
                ";", "'{'"
            ]
        );
        assert_eq!(
            read("let s = \"a, b\" + r#\"c \"d\"\"#;"),
            ["let", "s", "=", "\"a, b\"", "+", "r#\"c \"d\"\"#", ";"]
        );
        assert_eq!(
            read("fn g<'a>(x: &'a Ñame)"),
            ["fn", "g", "<", "'", "a", ">", "(", "x", ":", "&", "'", "a", "Ñame", ")"]
        );
        assert_eq!(
            words("a, \"b c\" // d", None).len(),
            8,
            "a language this does not read has no strings or comments"
        );
    }

    #[test]
    fn what_stands_between_two_words_is_read_without_its_comments() {
        assert_eq!(
            uncommented(",\n    /// the first\n    ", Some(Language::Rust)),
            ",\n    \n    "
        );
        assert_eq!(uncommented(" /* a, b */ , ", Some(Language::Rust)), "  , ");
    }
}

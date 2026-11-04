//! Code as its language writes it: its comments, its literals and the holes
//! in them, and the names, numbers, signs and spaces of the code itself. One
//! reading serves every reader of code (the word diff, the colors of code,
//! the items a text declares, a terminal's page of Markdown, the engine's
//! search for a name and the smells' measure of code), so no two of them can
//! disagree on where a string, a comment or a fence ends.

mod literal;
#[cfg(test)]
mod tests;

use std::ops::Range;
use std::path::Path;

pub use literal::is_word;
pub(crate) use literal::{fence_left_open, Fences, MIN_FENCE};
use literal::{Close, Escape, Hole, Literal};

/// A language the lexer reads, by how its source writes comments and
/// literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    /// C and C++, which write comments and literals alike.
    C,
    Java,
    Go,
    Kotlin,
    Scala,
    Swift,
    CSharp,
    Dart,
    /// JavaScript and TypeScript, which write comments and literals alike.
    Script,
    Python,
    Shell,
    Yaml,
    Toml,
    Json,
    Markdown,
}

/// Every extension the lexer knows: the language a file of it is written
/// in, and the name a Markdown fence gives its code so a viewer that colors
/// code colors it right.
const EXTENSIONS: &[(&str, Language, &str)] = &[
    ("rs", Language::Rust, "rust"),
    ("c", Language::C, "c"),
    ("h", Language::C, "c"),
    ("cc", Language::C, "cpp"),
    ("cpp", Language::C, "cpp"),
    ("hpp", Language::C, "cpp"),
    ("java", Language::Java, "java"),
    ("go", Language::Go, "go"),
    ("kt", Language::Kotlin, "kotlin"),
    ("kts", Language::Kotlin, "kotlin"),
    ("scala", Language::Scala, "scala"),
    ("swift", Language::Swift, "swift"),
    ("cs", Language::CSharp, "csharp"),
    ("dart", Language::Dart, "dart"),
    ("js", Language::Script, "javascript"),
    ("jsx", Language::Script, "jsx"),
    ("mjs", Language::Script, "javascript"),
    ("cjs", Language::Script, "javascript"),
    ("ts", Language::Script, "typescript"),
    ("tsx", Language::Script, "tsx"),
    ("mts", Language::Script, "typescript"),
    ("cts", Language::Script, "typescript"),
    ("py", Language::Python, "python"),
    ("sh", Language::Shell, "sh"),
    ("bash", Language::Shell, "bash"),
    ("zsh", Language::Shell, "zsh"),
    ("yaml", Language::Yaml, "yaml"),
    ("yml", Language::Yaml, "yaml"),
    ("toml", Language::Toml, "toml"),
    ("json", Language::Json, "json"),
    ("md", Language::Markdown, "markdown"),
    ("markdown", Language::Markdown, "markdown"),
];

impl Language {
    /// The language of a file whose extension is `extension`, when the
    /// lexer knows one.
    pub fn of_extension(extension: &str) -> Option<Language> {
        EXTENSIONS
            .iter()
            .find(|(known, ..)| *known == extension)
            .map(|&(_, language, _)| language)
    }

    /// The language of the file at `path`, by its extension.
    pub fn of(path: &str) -> Option<Language> {
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(Language::of_extension)
    }

    /// The language a Markdown fence's info string names: its first word, a
    /// fence name or an extension, as `rust` or `rs`, `typescript` or `ts`.
    pub(crate) fn named(info: &str) -> Option<Language> {
        let name = info
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .next()?;
        EXTENSIONS
            .iter()
            .find(|(extension, _, fence)| *fence == name || *extension == name)
            .map(|&(_, language, _)| language)
    }

    /// The extensions of the files written in the language a file whose
    /// extension is `extension` is: every one the language's files carry,
    /// `extension` among them. An extension of no language the lexer knows
    /// is a family of its own, `extension` alone.
    pub fn family(extension: &str) -> Vec<&str> {
        match Language::of_extension(extension) {
            Some(language) => EXTENSIONS
                .iter()
                .filter(|(_, of, _)| *of == language)
                .map(|&(known, ..)| known)
                .collect(),
            None => vec![extension],
        }
    }

    /// Whether a quoted word stops being code. In a program's source a
    /// string's text is data the program holds; in shell, YAML, TOML, JSON
    /// and Markdown a word quoted is the same word unquoted.
    fn quoting_hides(self) -> bool {
        !matches!(
            self,
            Language::Shell | Language::Yaml | Language::Toml | Language::Json | Language::Markdown
        )
    }
}

/// The name a Markdown fence gives the code of the file at `path`, by its
/// extension, so a viewer that colors code colors it right; empty for a
/// file of no language the lexer knows.
pub fn fence(path: &str) -> &'static str {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|extension| EXTENSIONS.iter().find(|(known, ..)| *known == extension))
        .map_or("", |&(_, _, fence)| fence)
}

/// What a stretch of source is to a reader of code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A run of letters, digits and `_` that starts with a letter or `_`: a
    /// keyword or a name.
    Name,
    /// A number: a run that starts with a digit, with its letters, its `_`
    /// and each `.` a digit follows.
    Number,
    /// A literal's text, its quotes and its prefix included, up to a hole in
    /// it or through its close.
    Text,
    /// A comment, its markers included.
    Comment,
    /// A run of whitespace, line breaks included.
    Space,
    /// Any other single character: a sign, a bracket, a comma.
    Sign,
}

/// A stretch of source, by what it is and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: Kind,
    pub(crate) range: Range<usize>,
}

/// The tokens of `source`, code in `language`, in order: every byte of it is
/// in exactly one. The code in a literal's hole is read as code, between two
/// pieces of the literal's text: `` `a${b}c` `` is a text, a name and a
/// text. With no language, nothing is a comment or a literal.
pub(crate) fn tokens(source: &str, language: Option<Language>) -> Vec<Token> {
    let mut tokens = Vec::new();
    walk(source, language, &mut |kind, range| {
        tokens.push(Token { kind, range });
    });
    tokens
}

/// `source`, code in `language`, with each comment turned to spaces, and the
/// text of each literal too where quoting hides a word, its line breaks
/// kept: what is left is the code, every byte where it stood. The code in a
/// literal's hole stays. With no language, `source` comes back as it is.
pub fn masked(source: &str, language: Option<Language>) -> String {
    let hides = language.is_some_and(Language::quoting_hides);
    let mut out = String::with_capacity(source.len());
    walk(source, language, &mut |kind, range| {
        let piece = &source[range];
        match kind {
            Kind::Comment => blank(&mut out, piece),
            Kind::Text if hides => blank(&mut out, piece),
            _ => out.push_str(piece),
        }
    });
    out
}

/// `piece` written onto `out` as a space for each of its bytes, its line
/// breaks kept, so every byte after it stays where it stood.
fn blank(out: &mut String, piece: &str) {
    out.extend(piece.bytes().map(|byte| match byte {
        b'\n' => '\n',
        _ => ' ',
    }));
}

/// The words of `text`, code in `language`, by where each stands: a name, a
/// number, a whole literal's text or a piece of it, or any other single
/// sign. Whitespace, comments and commas are not words: a comma separates,
/// and where one stands is how a formatter writes it.
pub(crate) fn words(text: &str, language: Option<Language>) -> Vec<Range<usize>> {
    tokens(text, language)
        .into_iter()
        .filter(|token| match token.kind {
            Kind::Space | Kind::Comment => false,
            Kind::Sign => &text[token.range.clone()] != ",",
            Kind::Name | Kind::Number | Kind::Text => true,
        })
        .map(|token| token.range)
        .collect()
}

/// `gap`, what stands between two words of code in `language`, without its
/// comments: the whitespace and the commas a reader sees between them.
pub(crate) fn uncommented(gap: &str, language: Option<Language>) -> String {
    tokens(gap, language)
        .into_iter()
        .filter(|token| token.kind != Kind::Comment)
        .map(|token| &gap[token.range])
        .collect()
}

/// The code spans of `prose`, Markdown text no fence opens: each a run of
/// backticks through the run of as many that closes it.
pub(crate) fn code_spans(prose: &str) -> Vec<Range<usize>> {
    tokens(prose, Some(Language::Markdown))
        .into_iter()
        .filter(|token| token.kind == Kind::Text)
        .map(|token| token.range)
        .collect()
}

/// The level of the heading a line of Markdown opens, read from its first
/// `#`: one to six `#` that a space, a tab or the line's end follows, as
/// CommonMark reads a heading's marks. Where the line stands in its document
/// is the caller's to read.
pub(crate) fn heading_level(line: &str) -> Option<usize> {
    let level = line.bytes().take_while(|&byte| byte == b'#').count();
    let rest = &line[level..];
    let blank = rest.is_empty() || rest.starts_with([' ', '\t', '\r']);
    ((1..=6).contains(&level) && blank).then_some(level)
}

/// The deepest one piece of code is read inside another: a literal's hole
/// read as code, or a Markdown fence's code read in its own language.
/// Deeper than code nests them, and shallow enough that a text nesting them
/// by the thousand cannot exhaust a thread's stack. A hole past it reads as
/// the text around it, and a fence's code as plain.
pub(crate) const DEEPEST: usize = 64;

/// Reads `source`, code in `language`, handing each token to `emit` in order.
fn walk(source: &str, language: Option<Language>, emit: &mut dyn FnMut(Kind, Range<usize>)) {
    Walk {
        source,
        language,
        fences: Fences::in_markdown(source, language),
        emit,
        nested: 0,
    }
    .code(0, None);
}

/// One reading of a source, and where its tokens go.
struct Walk<'a, 'e> {
    source: &'a str,
    language: Option<Language>,
    /// The fences of a Markdown source, read once for the whole of it.
    fences: Fences,
    emit: &'e mut dyn FnMut(Kind, Range<usize>),
    /// How many holes deep the reading stands.
    nested: usize,
}

impl Walk<'_, '_> {
    /// Hands on the token `kind` at `range`, unless it holds nothing.
    fn emit(&mut self, kind: Kind, range: Range<usize>) {
        if !range.is_empty() {
            (self.emit)(kind, range);
        }
    }

    /// Reads code from `at` to the end of the source, or, in a literal's
    /// hole, up to the `close` that ends the hole, which it leaves for the
    /// literal's text; answers where it stopped. A hole a bracket closes
    /// counts the brackets opened inside it; one its line's break closes
    /// ends at the first.
    fn code(&mut self, mut at: usize, close: Option<u8>) -> usize {
        let bytes = self.source.as_bytes();
        let open = close.and_then(|close| match close {
            b')' => Some(b'('),
            b'}' => Some(b'{'),
            _ => None,
        });
        let mut depth = 0usize;
        while let Some(&byte) = bytes.get(at) {
            if let Some(language) = self.language {
                if let Some(end) = literal::comment(language, bytes, at) {
                    self.emit(Kind::Comment, at..end);
                    at = end;
                    continue;
                }
                if let Some(found) = literal::literal(language, bytes, at, &self.fences) {
                    at = self.literal(at, found);
                    continue;
                }
            }
            if Some(byte) == close && depth == 0 {
                return at;
            }
            if Some(byte) == open {
                depth += 1;
            } else if Some(byte) == close {
                depth -= 1;
            }
            at = self.word(at, close);
        }
        at
    }

    /// Reads the literal that opens at `at`: its text up to each hole, the
    /// hole's code as code, and the rest of its text through its close, or
    /// up to its line's break where its text does not run on past one;
    /// answers where it ends.
    fn literal(&mut self, at: usize, literal: Literal) -> usize {
        let bytes = self.source.as_bytes();
        let end = match &literal.close {
            Close::At(end) => (*end).min(bytes.len()),
            Close::Bytes(_) => bytes.len(),
        };
        let mut text = at;
        let mut index = literal.text;
        while index < end {
            // A hole past the deepest reads as the text around it, its
            // opening passed over whole, so a long run of braces is read once.
            let hole = literal.holes.at(bytes, index).map(|hole| match hole {
                Hole::Code { from, .. } if self.nested >= DEEPEST => Hole::Text {
                    to: from.max(index + 1),
                },
                hole => hole,
            });
            if let Some(hole) = hole {
                (index, text) = self.read_hole(hole, text);
                continue;
            }
            if let Close::Bytes(close) = &literal.close {
                if literal::bytes_from(bytes, index).starts_with(close) {
                    let after = index + close.len();
                    let doubled = literal.escape == Escape::Doubled
                        && literal::bytes_from(bytes, after).starts_with(close);
                    if !doubled {
                        self.emit(Kind::Text, text..after);
                        return after;
                    }
                    index = after + close.len();
                    continue;
                }
            }
            if !literal.lines && bytes[index] == b'\n' {
                self.emit(Kind::Text, text..index);
                return index;
            }
            index += match literal.escape == Escape::Backslash && bytes[index] == b'\\' {
                true => 2,
                false => 1,
            };
        }
        let stop = index.clamp(end, bytes.len());
        self.emit(Kind::Text, text..stop);
        stop
    }

    /// Reads the hole `hole` in a literal's text, whose text not yet handed
    /// on starts at `text`: code is read as code, a name becomes its token,
    /// and text that only looks like a hole is passed over. Answers where the
    /// literal goes on from, and where its text not yet handed on starts.
    fn read_hole(&mut self, hole: Hole, text: usize) -> (usize, usize) {
        match hole {
            Hole::Code { from, close } => {
                self.emit(Kind::Text, text..from);
                self.nested += 1;
                let end = self.code(from, Some(close));
                self.nested -= 1;
                (end, end)
            }
            Hole::Name { from, to } => {
                self.emit(Kind::Text, text..from);
                self.emit(Kind::Name, from..to);
                (to, to)
            }
            Hole::Text { to } => (to, text),
        }
    }

    /// Reads the name, the number, the run of space or the sign that starts
    /// at `at`; answers where it ends. A run of space stops at the `close` of
    /// the hole it is read in, which the hole leaves to its literal. Every
    /// stop the walk makes is a char boundary, since each literal, comment
    /// and hole opens and ends on an ASCII byte or at the source's end; a
    /// debug build asserts it, and a release build ends the walk there rather
    /// than cut a char.
    fn word(&mut self, at: usize, close: Option<u8>) -> usize {
        debug_assert!(
            self.source.is_char_boundary(at),
            "the walk stops on a char boundary"
        );
        let Some(rest) = self.source.get(at..) else {
            return self.source.len();
        };
        let Some(first) = rest.chars().next() else {
            return self.source.len();
        };
        let named = |ch: char| ch.is_alphanumeric() || ch == '_';
        let (kind, length) = if first.is_whitespace() {
            let ends = |ch: char| {
                !ch.is_whitespace() || close.is_some_and(|close| ch == char::from(close))
            };
            let after = &rest[first.len_utf8()..];
            (
                Kind::Space,
                first.len_utf8() + after.find(ends).unwrap_or(after.len()),
            )
        } else if first.is_ascii_digit() {
            (Kind::Number, number(rest))
        } else if named(first) {
            (
                Kind::Name,
                rest.find(|ch: char| !named(ch)).unwrap_or(rest.len()),
            )
        } else {
            (Kind::Sign, first.len_utf8())
        };
        self.emit(kind, at..at + length);
        at + length
    }
}

/// The length of the number that starts `rest`: its digits, its letters and
/// its `_`, and each `.` a digit follows, so `1.0` and `0x1F` are one number
/// each and `1..2` is two.
fn number(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let mut end = 0;
    while let Some(&byte) = bytes.get(end) {
        let goes_on = byte.is_ascii_alphanumeric()
            || byte == b'_'
            || (byte == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit));
        if !goes_on {
            break;
        }
        end += 1;
    }
    end
}

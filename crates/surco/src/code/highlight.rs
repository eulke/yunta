//! What each word of code is to a reader of code: a keyword, a type, a
//! function, the text of a string, a number, a comment, an attribute or a
//! macro. It is read from the one lexer's tokens and the words each
//! language reserves, so every surface that colors code colors it by the
//! same reading, and coloring never changes a word.

#[cfg(test)]
mod tests;
mod words;

use std::ops::Range;

use super::lex::{heading_level, tokens, Fences, Kind, Language, Token, DEEPEST};
use crate::line::{Line, Syntax, Tone};

/// The stretches of `text`, code in `language`, a reader of code sees
/// colored, in order and apart, each with what it is; a stretch not listed
/// reads plain. With no language, none is colored.
pub(crate) fn highlighted(text: &str, language: Option<Language>) -> Vec<(Range<usize>, Syntax)> {
    within(text, language, 0)
}

/// What [`highlighted`] colors in `text`, read as the code of a fence
/// `depth` fences deep.
fn within(text: &str, language: Option<Language>, depth: usize) -> Vec<(Range<usize>, Syntax)> {
    let Some(language) = language else {
        return Vec::new();
    };
    let tokens = tokens(text, Some(language));
    let (closes, inside) = match_brackets(text, &tokens);
    let reading = Reading {
        text,
        language,
        fences: Fences::in_markdown(text, Some(language)),
        tokens: &tokens,
        closes,
        inside,
        depth,
    };
    let mut colored = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let (stretches, next) = reading.read(at);
        colored.extend(stretches);
        at = next;
    }
    colored
}

/// The stretch `range` of `text` as a line: each part of it that `colored`
/// lists in its word's tone, and the rest plain. A stretch of `colored` that
/// runs past `range`, as a string over lines does, colors only the part of it
/// inside. `colored` is in order and apart, so the first stretch the range
/// holds is found by search.
pub(crate) fn painted(text: &str, range: Range<usize>, colored: &[(Range<usize>, Syntax)]) -> Line {
    let mut line = Line::new();
    let mut at = range.start;
    let first = colored.partition_point(|(stretch, _)| stretch.end <= range.start);
    let inside = colored[first..]
        .iter()
        .take_while(|(stretch, _)| stretch.start < range.end);
    for (stretch, syntax) in inside {
        let from = stretch.start.max(range.start);
        let to = stretch.end.min(range.end);
        if at < from {
            line = line.plain(&text[at..from]);
        }
        line = line.push(Tone::Code(*syntax), &text[from..to]);
        at = to;
    }
    if at < range.end {
        line = line.plain(&text[at..range.end]);
    }
    line
}

/// What one token, or a few read together, color: the stretches, and the
/// index of the token after them.
type Read = (Vec<(Range<usize>, Syntax)>, usize);

/// A text's tokens, read for what each is to a reader of code.
struct Reading<'a> {
    text: &'a str,
    language: Language,
    /// The fences of a Markdown text, read once for the whole of it.
    fences: Fences,
    tokens: &'a [Token],
    /// For each token that is a `[`, the token of the `]` that closes it.
    closes: Vec<Option<usize>>,
    /// For each token, how many `[` stand open before it.
    inside: Vec<usize>,
    /// How many fences deep the text stands in the text that holds it.
    depth: usize,
}

impl Reading<'_> {
    /// What token `at` is, with any token after it that it colors as one.
    fn read(&self, at: usize) -> Read {
        match self.tokens[at].kind {
            Kind::Space => (Vec::new(), at + 1),
            Kind::Comment => self.one(at, Syntax::Comment),
            Kind::Number => self.one(at, Syntax::Number),
            Kind::Text => self.text(at),
            Kind::Sign => self.sign(at),
            Kind::Name => self.name(at),
        }
    }

    /// The text token `at` covers.
    fn said(&self, at: usize) -> &str {
        &self.text[self.tokens[at].range.clone()]
    }

    /// Whether token `at` is there and is the sign `sign`.
    fn is_sign(&self, at: Option<usize>, sign: &str) -> bool {
        at.and_then(|at| self.tokens.get(at)).is_some_and(|token| {
            token.kind == Kind::Sign && &self.text[token.range.clone()] == sign
        })
    }

    /// Whether token `at` is there and is a name.
    fn is_name(&self, at: usize) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.kind == Kind::Name)
    }

    /// The nearest token before `at` that is code, past spaces and comments.
    fn before(&self, at: usize) -> Option<usize> {
        (0..at).rev().find(|&index| is_code(&self.tokens[index]))
    }

    /// The nearest token after `at` that is code, past spaces and comments.
    fn after(&self, at: usize) -> Option<usize> {
        (at + 1..self.tokens.len()).find(|&index| is_code(&self.tokens[index]))
    }

    /// Whether token `at` is the first thing on its line but space: no code
    /// and no comment stands before it there.
    fn opens_line(&self, at: usize) -> bool {
        (0..at)
            .rev()
            .find(|&index| self.tokens[index].kind != Kind::Space)
            .is_none_or(|before| {
                self.text[self.tokens[before].range.end..self.tokens[at].range.start].contains('\n')
            })
    }

    /// Token `at` alone, colored as `syntax`.
    fn one(&self, at: usize, syntax: Syntax) -> Read {
        (vec![(self.tokens[at].range.clone(), syntax)], at + 1)
    }

    /// Tokens `at` through `end`, colored as one `syntax`.
    fn through(&self, at: usize, end: usize, syntax: Syntax) -> Read {
        let stretch = self.tokens[at].range.start..self.tokens[end].range.end;
        (vec![(stretch, syntax)], end + 1)
    }

    /// Token `at`, plain.
    fn plain(&self, at: usize) -> Read {
        (Vec::new(), at + 1)
    }

    /// A literal's text: a key in JSON, a fence or a code span in Markdown,
    /// and a string's text everywhere else.
    fn text(&self, at: usize) -> Read {
        match self.language {
            Language::Json if self.is_sign(self.after(at), ":") => self.one(at, Syntax::Keyword),
            Language::Markdown => (self.fence(at), at + 1),
            _ => self.one(at, Syntax::Text),
        }
    }

    /// A Markdown literal: a fence, read as the lexer reads one, whose
    /// opening and closing lines are markers, read as comments, and whose
    /// code takes the tones of the language its info string names, plain
    /// where it names none or where fences nest past the deepest; and any
    /// other, a code span, a string's text.
    fn fence(&self, at: usize) -> Vec<(Range<usize>, Syntax)> {
        let range = self.tokens[at].range.clone();
        let Some(fence) = self.fences.at(range.start) else {
            return vec![(range, Syntax::Text)];
        };
        let (opening, code) = (fence.opening.clone(), fence.code.clone());
        let inner = match self.depth < DEEPEST {
            true => within(
                &self.text[code.clone()],
                Language::named(&self.text[fence.info.clone()]),
                self.depth + 1,
            ),
            false => Vec::new(),
        };
        let inner = inner.into_iter().map(|(stretch, syntax)| {
            (code.start + stretch.start..code.start + stretch.end, syntax)
        });
        std::iter::once((opening, Syntax::Comment))
            .chain(inner)
            .chain(
                fence
                    .closing
                    .clone()
                    .map(|closing| (closing, Syntax::Comment)),
            )
            .filter(|(stretch, _)| !stretch.is_empty())
            .collect()
    }

    /// A sign: where an attribute, a preprocessor's directive, a heading, a
    /// table's header or a flag opens, all of it; plain otherwise.
    fn sign(&self, at: usize) -> Read {
        match (self.language, self.said(at)) {
            (Language::Rust, "#") => self.rust_attribute(at),
            (Language::C, "#") if self.opens_line(at) => self.directive(at),
            (Language::Markdown, "#") if self.opens_line(at) => self.heading(at),
            (Language::Toml, "[") if self.opens_line(at) && self.inside[at] == 0 => self.table(at),
            (Language::Shell, "-") if self.starts_word(at) => self.flag(at),
            (language, "@") if words::decorates(language) => self.decorator(at),
            _ => self.plain(at),
        }
    }

    /// `#[…]` or `#![…]`, through the bracket that closes it; plain where none
    /// does.
    fn rust_attribute(&self, at: usize) -> Read {
        let open = match self.is_sign(Some(at + 1), "!") {
            true => at + 2,
            false => at + 1,
        };
        match self.closes.get(open).copied().flatten() {
            Some(close) => self.through(at, close, Syntax::Attribute),
            None => self.plain(at),
        }
    }

    /// A preprocessor's directive, `#include` or `#define`: the `#` and the
    /// name after it.
    fn directive(&self, at: usize) -> Read {
        match self.after(at).filter(|&name| self.is_name(name)) {
            Some(name) => self.through(at, name, Syntax::Attribute),
            None => self.plain(at),
        }
    }

    /// A Markdown heading, through its line's end: one `#` reads as a type,
    /// two or more as a keyword. A `#` that opens no heading, as
    /// [`heading_level`] reads one, is plain.
    fn heading(&self, at: usize) -> Read {
        let start = self.tokens[at].range.start;
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |found| start + found);
        let Some(level) = heading_level(&self.text[start..end]) else {
            return self.plain(at);
        };
        let syntax = match level {
            1 => Syntax::Type,
            _ => Syntax::Keyword,
        };
        let last = (at..self.tokens.len())
            .take_while(|&index| self.tokens[index].range.start < end)
            .last()
            .unwrap_or(at);
        (vec![(start..end, syntax)], last + 1)
    }

    /// A TOML table's header, `[name]` or `[[name]]`, through the bracket
    /// that closes it on its line, read as a type. A `[` inside a value's
    /// array opens no header.
    fn table(&self, at: usize) -> Read {
        let closes = (at + 1..self.tokens.len())
            .take_while(|&index| !self.said(index).contains('\n'))
            .filter(|&index| self.is_sign(Some(index), "]"))
            .last();
        match closes {
            Some(end) => self.through(at, end, Syntax::Type),
            None => self.plain(at),
        }
    }

    /// A shell flag: the dashes at a word's start and the name after them,
    /// with its own dashes and names, as `--dry-run`.
    fn flag(&self, at: usize) -> Read {
        let mut end = at;
        while self.is_sign(Some(end + 1), "-") {
            end += 1;
        }
        if !self.is_name(end + 1) {
            return self.plain(at);
        }
        end += 1;
        while self.is_sign(Some(end + 1), "-") && self.is_name(end + 2) {
            end += 2;
        }
        self.through(at, end, Syntax::Keyword)
    }

    /// A decorator or an annotation: the `@` and the name after it, with the
    /// names its dots join.
    fn decorator(&self, at: usize) -> Read {
        if !self.is_name(at + 1) {
            return self.plain(at);
        }
        let mut end = at + 1;
        while self.is_sign(Some(end + 1), ".") && self.is_name(end + 2) {
            end += 2;
        }
        self.through(at, end, Syntax::Attribute)
    }

    /// Whether a word starts at token `at`: nothing stands before it, or a
    /// space does.
    fn starts_word(&self, at: usize) -> bool {
        at.checked_sub(1)
            .is_none_or(|before| self.tokens[before].kind == Kind::Space)
    }

    /// Whether token `at` names a member: a `.` stands right before it, a dot
    /// of its own, so a spread's `...` or a range's `..` opens no member.
    fn member(&self, at: usize) -> bool {
        at.checked_sub(1).is_some_and(|dot| {
            self.is_sign(Some(dot), ".") && !self.is_sign(dot.checked_sub(1), ".")
        })
    }

    /// A name: a word its language reserves, or, where it names a member, a
    /// word the language writes as a keyword after a `.`; or what the words
    /// around it make of it.
    fn name(&self, at: usize) -> Read {
        let said = self.said(at);
        let keyword = match self.member(at) {
            true => words::keyword_after_dot(self.language, said),
            false => words::reserved(self.language, said),
        };
        if keyword {
            return self.one(at, Syntax::Keyword);
        }
        match self.language {
            Language::Shell if self.starts_command(at) && !self.is_sign(self.after(at), "=") => {
                self.one(at, Syntax::Function)
            }
            Language::Yaml if self.opens_entry(at) => self.key(at, ":"),
            Language::Toml if self.opens_line(at) => self.key(at, "="),
            Language::Shell
            | Language::Yaml
            | Language::Toml
            | Language::Json
            | Language::Markdown => self.plain(at),
            _ => self.program_name(at),
        }
    }

    /// A name in a program: a Rust macro and its `!`; the name a declaration
    /// gives, or one a call calls, a function; a type the language names,
    /// where it names no member, or a name that opens with a capital, a type,
    /// after a `.` too, as `http.Request` names one. Go and C# capitalize
    /// their functions, so there a capital a call follows is a function.
    fn program_name(&self, at: usize) -> Read {
        let said = self.said(at);
        let macro_bang = self.language == Language::Rust
            && self.is_sign(Some(at + 1), "!")
            && !self.is_sign(Some(at + 2), "=");
        if macro_bang {
            return self.through(at, at + 1, Syntax::Macro);
        }
        let declared = self
            .before(at)
            .is_some_and(|before| words::declares(self.language, self.said(before)));
        let called = self.is_sign(self.after(at), "(");
        let capital = said.starts_with(|ch: char| ch.is_ascii_uppercase());
        let capital_calls = matches!(self.language, Language::Go | Language::CSharp);
        if declared || (called && (capital_calls || !capital)) {
            return self.one(at, Syntax::Function);
        }
        if capital || (!self.member(at) && words::typed(self.language, said)) {
            return self.one(at, Syntax::Type);
        }
        self.plain(at)
    }

    /// Whether a shell command starts at token `at`: it opens its line, or
    /// it follows a pipe, a `;`, a `&`, an opening `(` or `{`, a word after
    /// which a command comes (`then`, `do`, `else`, `if`…), or the `$(` that
    /// opens a hole.
    fn starts_command(&self, at: usize) -> bool {
        if self.opens_line(at) {
            return true;
        }
        let Some(before) = self.before(at) else {
            return true;
        };
        let said = self.said(before);
        match self.tokens[before].kind {
            Kind::Sign => matches!(said, "|" | ";" | "&" | "(" | "{"),
            Kind::Name => matches!(
                said,
                "then" | "do" | "else" | "elif" | "if" | "while" | "until" | "time" | "exec"
            ),
            Kind::Text => said.ends_with("$("),
            _ => false,
        }
    }

    /// Whether a YAML entry opens at token `at`: it is the first code on its
    /// line, or it follows a `-` that is.
    fn opens_entry(&self, at: usize) -> bool {
        self.opens_line(at)
            || self
                .before(at)
                .is_some_and(|before| self.is_sign(Some(before), "-") && self.opens_line(before))
    }

    /// A key that opens an entry: the names, numbers, dots and dashes from
    /// token `at` to the `separator` that ends it on its line, `:` in YAML
    /// and `=` in TOML, read as a keyword. In YAML the `:` is one a space, a
    /// line break or the end follows, so `http://` ends no key.
    fn key(&self, at: usize, separator: &str) -> Read {
        let mut end = at;
        for next in at + 1..self.tokens.len() {
            let said = self.said(next);
            match self.tokens[next].kind {
                Kind::Sign if said == separator && (separator != ":" || self.ends_key(next)) => {
                    return self.through(at, end, Syntax::Keyword);
                }
                Kind::Name | Kind::Number => end = next,
                Kind::Sign if said == "." || said == "-" => end = next,
                Kind::Space if !said.contains('\n') => {}
                _ => break,
            }
        }
        self.plain(at)
    }

    /// Whether the `:` at token `at` ends a YAML key: a space, a line break or
    /// the end of the text follows it.
    fn ends_key(&self, at: usize) -> bool {
        self.text[self.tokens[at].range.end..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    }
}

/// Whether `token` is code: neither space nor a comment.
fn is_code(token: &Token) -> bool {
    !matches!(token.kind, Kind::Space | Kind::Comment)
}

/// The brackets of `tokens`, matched once: for each token that is a `[`, the
/// token of the `]` that closes it, so an attribute left open costs no more
/// than one closed; and for each token, how many `[` stand open before it.
fn match_brackets(text: &str, tokens: &[Token]) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut closes = vec![None; tokens.len()];
    let mut inside = vec![0; tokens.len()];
    let mut open = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        inside[index] = open.len();
        match (token.kind, &text[token.range.clone()]) {
            (Kind::Sign, "[") => open.push(index),
            (Kind::Sign, "]") => {
                if let Some(opened) = open.pop() {
                    closes[opened] = Some(index);
                }
            }
            _ => {}
        }
    }
    (closes, inside)
}

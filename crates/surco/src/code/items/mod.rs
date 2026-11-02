//! The items a text declares, read in its language: Rust, Go, JavaScript
//! or TypeScript, and Python. A text is read on its code alone, each
//! string and comment turned to spaces where it stands, so a brace, a `;`
//! or an indent inside one never opens or ends an item; what is read is
//! cut from the text at the offsets the reading found. The engine reads
//! what a plan's shapes are in the tree with it, and the fold of a test
//! file finds each function with it.

mod lines;
mod rust;
#[cfg(test)]
mod tests;

use std::ops::Range;

use super::lex::is_word;
use super::Language;

/// Whether `byte` belongs to a name: a letter, a digit, `_`, or a byte of a
/// character past ASCII, which a name may hold.
fn named(byte: u8) -> bool {
    is_word(byte) || !byte.is_ascii()
}

/// The byte-order mark a file may open with, U+FEFF: a text that opens with
/// it reads the mark as blank, so that it hides no declaration and makes no
/// indent.
const BYTE_ORDER_MARK: &str = "\u{feff}";

/// The kinds of item a text declares, each by the word that declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A function or a method: Rust's `fn`, Go's `func`, a script's
    /// `function` or a name bound to one, Python's `def`.
    Fn,
    Struct,
    Enum,
    Union,
    Trait,
    Impl,
    /// A type alias, or what Go's `type` declares.
    Type,
    Const,
    Static,
    /// A module: Rust's inline `mod`, a script's `namespace`.
    Mod,
    /// Rust's `macro_rules!`.
    Macro,
    /// A class, in a script or in Python.
    Class,
    /// A script's `interface`.
    Interface,
    /// A variable: a script's `let` or `var`, Go's `var`.
    Var,
}

impl Kind {
    /// Whether the item holds members a shape names one by one.
    pub fn holds_members(self) -> bool {
        matches!(
            self,
            Kind::Impl | Kind::Trait | Kind::Mod | Kind::Class | Kind::Interface
        )
    }
}

/// One item of a text, by the offsets it stands at.
pub struct Item {
    pub kind: Kind,
    /// What it is called: an `impl` by its head, a Go method by its
    /// receiver's type and its name, as `Store.Load`.
    pub name: String,
    /// Where it starts: at its first attribute or decorator, or at its head.
    pub start: usize,
    /// Where the words that declare it start, past its attributes.
    pub head: usize,
    pub bounds: Bounds,
    /// Whether it is a method that takes its receiver as a parameter: Rust's
    /// `self`, Python's `self` or `cls`.
    pub receiver: bool,
    /// The keyword a group writes once for every item it declares, `var` of a
    /// Go `var (`, where the item stands in one.
    pub lead: Option<Range<usize>>,
}

impl Item {
    /// Whether it is of `other`'s kind and called by `other`'s name.
    pub fn is(&self, other: &Item) -> bool {
        self.kind == other.kind && self.name == other.name
    }

    /// Whether `other` stands within its body.
    pub fn holds(&self, other: &Item) -> bool {
        self.bounds
            .body
            .as_ref()
            .is_some_and(|body| body.contains(&other.start))
    }
}

/// Where an item's head stops, what its body holds, and where it ends.
pub struct Bounds {
    /// At the `{` or the `:` that opens its body; for an item with no body,
    /// at the `;` that ends it, or just past its last word.
    pub stop: usize,
    /// What its body holds, when it has one: what its braces hold, or what
    /// follows a Python head's `:`.
    pub body: Option<Range<usize>>,
    /// Just past it: past its body's `}` and a `;` right after that, past
    /// its `;`, or past its last word.
    pub end: usize,
}

/// A text in a language whose items are read, and its code: the same
/// bytes, each string and comment turned to spaces, its line breaks kept.
pub struct Text<'a> {
    original: &'a str,
    code: Vec<u8>,
    language: Language,
}

impl<'a> Text<'a> {
    /// `original`, code in `language`, where the items of `language` are
    /// read: Rust, Go, JavaScript or TypeScript, and Python.
    pub fn of(original: &'a str, language: Language) -> Option<Text<'a>> {
        let read = matches!(
            language,
            Language::Rust | Language::Go | Language::Script | Language::Python
        );
        read.then(|| {
            let mut code = super::masked(original, Some(language)).into_bytes();
            // A byte-order mark opens a file and is no word of it.
            if code.starts_with(BYTE_ORDER_MARK.as_bytes()) {
                code[..BYTE_ORDER_MARK.len()].fill(b' ');
            }
            Text {
                original,
                code,
                language,
            }
        })
    }

    /// The items the text declares at its top, in order.
    pub fn items(&self) -> Vec<Item> {
        self.read(0..self.code.len(), None)
    }

    /// The items `holder`'s body holds, in order, read as its members: a
    /// script's class declares its methods without `function`.
    pub fn inside(&self, holder: &Item) -> Vec<Item> {
        holder
            .bounds
            .body
            .clone()
            .map_or_else(Vec::new, |body| self.read(body, Some(holder.kind)))
    }

    /// The items the bodies of `holders` hold, in order.
    pub fn members<'i>(&self, holders: impl Iterator<Item = &'i Item>) -> Vec<Item> {
        holders.flat_map(|holder| self.inside(holder)).collect()
    }

    /// The first item the statement at `at`, or past the blank there,
    /// declares, read as a member of an item of `holder`'s kind where one
    /// holds it, and where the next statement may start.
    pub(crate) fn item_at(&self, at: usize, holder: Option<Kind>) -> (Option<Item>, usize) {
        let (items, next) = self.step(at, self.code.len(), holder);
        (items.into_iter().next(), next)
    }

    /// The language the text is read in.
    pub(crate) fn language(&self) -> Language {
        self.language
    }

    /// The text from `from` up to `stop`, without the blank and the `,` it
    /// ends with.
    pub fn signature(&self, from: usize, stop: usize) -> &'a str {
        let end = self.trimmed(from, stop);
        let comma = end > from && self.byte(end - 1) == Some(b',');
        let end = match comma {
            true => self.trimmed(from, end - 1),
            false => end,
        };
        self.slice(from, end)
    }

    /// The text from `from` to `to`, as the file writes it.
    pub fn slice(&self, from: usize, to: usize) -> &'a str {
        self.original.get(from..to).unwrap_or_default()
    }

    /// The attributes or decorators of `item`, as written but for their
    /// comments, their blanks collapsed: what tells one twin of a `cfg` pair
    /// from the other, the strings they hold included. Empty for an item that
    /// has none.
    pub fn attributes(&self, item: &Item) -> String {
        let written =
            super::lex::uncommented(self.slice(item.start, item.head), Some(self.language));
        written.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The words of `item` before its body, as code: an `impl`'s generics
    /// and its `where` among them.
    pub fn header(&self, item: &Item) -> String {
        self.words(item.head, item.bounds.stop)
    }

    /// The items within `span`, read as the members of an item of
    /// `holder`'s kind where one holds them.
    fn read(&self, span: Range<usize>, holder: Option<Kind>) -> Vec<Item> {
        let mut items = Vec::new();
        let mut at = span.start;
        while at < span.end {
            let (found, next) = self.step(at, span.end, holder);
            items.extend(found);
            at = next;
        }
        items
    }

    /// The items the statement at `at`, or past the blank there, declares
    /// before `limit`, and where the next statement may start: Rust's by
    /// its `;` and its braces, the other languages' by their lines.
    fn step(&self, at: usize, limit: usize, holder: Option<Kind>) -> (Vec<Item>, usize) {
        match self.language {
            Language::Rust => {
                let (item, next) = self.rust_item(at, limit);
                (item.into_iter().collect(), next)
            }
            _ => self.line_items(at, limit, holder),
        }
    }

    /// The code from `from` to `to`, its blanks collapsed.
    fn words(&self, from: usize, to: usize) -> String {
        let code = String::from_utf8_lossy(self.code.get(from..to).unwrap_or_default());
        code.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The bracket that closes the one that opens at `open`, before `limit`.
    fn closing(&self, open: usize, limit: usize) -> Option<usize> {
        let mut depth = 0usize;
        for at in open..limit {
            match self.byte(at)? {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(at);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Where the text from `from` to `to` ends once the blank it ends with
    /// is left out.
    fn trimmed(&self, from: usize, to: usize) -> usize {
        let code = self.code.get(from..to).unwrap_or_default();
        to - code
            .iter()
            .rev()
            .take_while(|b| b.is_ascii_whitespace())
            .count()
    }

    /// Past the blank that stands at `at`, before `limit`.
    fn past_blank(&self, at: usize, limit: usize) -> usize {
        let code = self.code.get(at..limit).unwrap_or_default();
        at + code.iter().take_while(|b| b.is_ascii_whitespace()).count()
    }

    /// The byte of code at `at`.
    fn byte(&self, at: usize) -> Option<u8> {
        self.code.get(at).copied()
    }
}

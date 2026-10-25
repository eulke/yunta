//! A Rust text split into the items it declares, at its top or within the
//! braces of a body. It is walked on its code alone, every string, char
//! and comment turned to spaces where it stands, so a brace in a string or
//! in a doc comment never opens or closes an item; what is read is cut
//! from the text at the offsets the walk found.

use std::ops::Range;

use super::super::blanked;
use super::super::syntax::{is_word, Lang};

/// The kinds of item a shape declares, each by the word that declares it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Fn,
    Struct,
    Enum,
    Union,
    Trait,
    Impl,
    Type,
    Const,
    Static,
    Mod,
    Macro,
}

impl Kind {
    /// The kind `word` declares, when it declares one.
    fn of(word: &str) -> Option<Kind> {
        Some(match word {
            "fn" => Kind::Fn,
            "struct" => Kind::Struct,
            "enum" => Kind::Enum,
            "union" => Kind::Union,
            "trait" => Kind::Trait,
            "impl" => Kind::Impl,
            "type" => Kind::Type,
            "const" => Kind::Const,
            "static" => Kind::Static,
            "mod" => Kind::Mod,
            "macro_rules" => Kind::Macro,
            _ => return None,
        })
    }

    /// Whether a body ends the item: a `const`, a `static` and a `type`
    /// end at their `;`, whatever block their value holds.
    fn ends_at_its_body(self) -> bool {
        !matches!(self, Kind::Const | Kind::Static | Kind::Type)
    }

    /// Whether the item holds members a shape names one by one.
    pub(super) fn holds_members(self) -> bool {
        matches!(self, Kind::Impl | Kind::Trait | Kind::Mod)
    }
}

/// One item of a text, by the offsets it stands at.
pub(super) struct Item {
    pub(super) kind: Kind,
    /// What it is called; an `impl`, by its head.
    name: String,
    /// Where it starts: at its first outer attribute, or at its head.
    pub(super) start: usize,
    /// Where the words that declare it start, past its attributes.
    pub(super) head: usize,
    pub(super) bounds: Bounds,
    /// Whether it is a function that takes `self`, which only a method
    /// does.
    pub(super) receiver: bool,
}

impl Item {
    /// Whether it is of `other`'s kind and called by `other`'s name.
    pub(super) fn is(&self, other: &Item) -> bool {
        self.kind == other.kind && self.name == other.name
    }

    /// Whether `other` stands within its body.
    pub(super) fn holds(&self, other: &Item) -> bool {
        self.bounds
            .body
            .as_ref()
            .is_some_and(|body| body.contains(&other.start))
    }
}

/// Where an item's head stops, what its body holds, and where it ends.
pub(super) struct Bounds {
    /// At the `{` that opens its body, or at the `;` that ends it.
    pub(super) stop: usize,
    /// What its body's braces hold, when a body ends it.
    pub(super) body: Option<Range<usize>>,
    /// Just past it: past its body's `}` and a `;` right after that, or
    /// past its `;`.
    pub(super) end: usize,
}

/// A text and its code: the same bytes, with every string, char and
/// comment turned to spaces, its line breaks kept.
pub(super) struct Text<'a> {
    original: &'a str,
    code: Vec<u8>,
}

impl<'a> Text<'a> {
    pub(super) fn of(original: &'a str) -> Text<'a> {
        Text {
            original,
            code: blanked(original.as_bytes(), Lang::Rust),
        }
    }

    /// Every offset of the text.
    pub(super) fn whole(&self) -> Range<usize> {
        0..self.code.len()
    }

    /// The items that stand within `span`, in order.
    pub(super) fn items(&self, span: Range<usize>) -> Vec<Item> {
        let mut items = Vec::new();
        let mut at = span.start;
        while at < span.end {
            let (item, next) = self.item_at(at, span.end);
            items.extend(item);
            at = next;
        }
        items
    }

    /// The items the bodies of `holders` hold, in order.
    pub(super) fn members<'i>(&self, holders: impl Iterator<Item = &'i Item>) -> Vec<Item> {
        holders
            .filter_map(|holder| holder.bounds.body.clone())
            .flat_map(|body| self.items(body))
            .collect()
    }

    /// The text from `from` up to `stop`, without the blank and the `,` it
    /// ends with.
    pub(super) fn signature(&self, from: usize, stop: usize) -> &'a str {
        let end = self.trimmed(from, stop);
        let comma = end > from && self.byte(end - 1) == Some(b',');
        let end = match comma {
            true => self.trimmed(from, end - 1),
            false => end,
        };
        self.slice(from, end)
    }

    /// The text from `from` to `to`, as the file writes it.
    pub(super) fn slice(&self, from: usize, to: usize) -> &'a str {
        self.original.get(from..to).unwrap_or_default()
    }

    /// The outer attributes of `item`, as code: what tells one twin of a
    /// `cfg` pair from the other. Empty for an item that has none.
    pub(super) fn attributes(&self, item: &Item) -> String {
        self.words(item.start, item.head)
    }

    /// The words of `item` before its body, as code: an `impl`'s generics
    /// and its `where` among them.
    pub(super) fn header(&self, item: &Item) -> String {
        self.words(item.head, item.bounds.stop)
    }

    /// The code from `from` to `to`, its blanks collapsed.
    fn words(&self, from: usize, to: usize) -> String {
        let code = String::from_utf8_lossy(self.code.get(from..to).unwrap_or_default());
        code.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The item that stands at `at` or past the blank there, before
    /// `limit`, and where the next may start. What declares no item a shape
    /// names is read past with none: a stray `;`, an inner attribute, a
    /// `use`, an `extern crate`, a `mod x;`, a macro's call.
    fn item_at(&self, at: usize, limit: usize) -> (Option<Item>, usize) {
        let start = self.past_blank(at, limit);
        match self.byte(start) {
            _ if start >= limit => (None, limit),
            Some(b'#') => match self.past_attribute(start, limit) {
                Some(past) if self.inner(start, limit) => (None, past),
                Some(_) => self.declared(start, limit),
                None => (None, start + 1),
            },
            Some(byte) if is_word(byte) => self.declared(start, limit),
            _ => (None, start + 1),
        }
    }

    /// The item whose attributes or words start at `start`, and where the
    /// next may start: `None` for one of no kind a shape declares.
    fn declared(&self, start: usize, limit: usize) -> (Option<Item>, usize) {
        let mut head = start;
        while let Some(past) = self.past_attribute(head, limit) {
            head = self.past_blank(past, limit);
        }
        let at = self.qualifiers(head, limit);
        let keyword = self
            .word(at)
            .and_then(|(word, after)| Some((Kind::of(word)?, after)));
        let blocks = keyword.is_none_or(|(kind, _)| kind.ends_at_its_body());
        let bounds = self.bounds(head, limit, blocks);
        let next = bounds.end.max(start + 1);
        let Some((kind, after)) = keyword else {
            return (None, next);
        };
        if kind == Kind::Mod && bounds.body.is_none() {
            return (None, next);
        }
        let Some(name) = self.name(kind, after, bounds.stop) else {
            return (None, next);
        };
        let receiver = kind == Kind::Fn && self.takes_self(after, bounds.stop);
        let item = Item {
            kind,
            name,
            start,
            head,
            bounds,
            receiver,
        };
        (Some(item), next)
    }

    /// Past the qualifiers that stand at `at`: `pub` and its `(…)`,
    /// `async`, `unsafe`, `extern` and the ABI after it, `default`, and a
    /// `const` a function's words follow.
    fn qualifiers(&self, mut at: usize, limit: usize) -> usize {
        while let Some((word, after)) = self.word(at) {
            let next = self.past_blank(after, limit);
            let past = match word {
                "pub" if self.byte(next) == Some(b'(') => {
                    self.closing(next, limit).map_or(limit, |close| close + 1)
                }
                "pub" | "async" | "unsafe" | "extern" | "default" => after,
                "const"
                    if matches!(
                        self.word(next),
                        Some(("fn" | "unsafe" | "extern" | "async", _))
                    ) =>
                {
                    after
                }
                _ => break,
            };
            at = self.past_blank(past, limit);
        }
        at
    }

    /// What the item of `kind` whose keyword ends at `after` is called: the
    /// word that follows it, past `mut` for a `static` and past `!` for a
    /// macro; for an `impl`, its head up to `stop`.
    fn name(&self, kind: Kind, after: usize, stop: usize) -> Option<String> {
        let at = self.past_blank(after, stop);
        let at = match (kind, self.byte(at)) {
            (Kind::Impl, _) => return Some(self.impl_head(at, stop)),
            (Kind::Macro, Some(b'!')) => self.past_blank(at + 1, stop),
            (Kind::Macro, _) => return None,
            _ => at,
        };
        let (word, end) = self.word(at)?;
        let (word, _) = match (kind, word) {
            (Kind::Static, "mut") => self.word(self.past_blank(end, stop))?,
            _ => (word, end),
        };
        Some(word.to_string())
    }

    /// The head of the `impl` whose words go on at `at`: past its generic
    /// parameters, up to `where` or `stop`, its blanks collapsed.
    fn impl_head(&self, at: usize, stop: usize) -> String {
        let from = self.past_generics(at, stop);
        let mut to = from;
        while to < stop {
            match self.word(to) {
                Some(("where", _)) => break,
                Some((_, end)) => to = end,
                None => to += 1,
            }
        }
        self.words(from, to)
    }

    /// Whether the function whose name follows the keyword that ends at
    /// `after` takes `self`: whether its parameters, past its `<…>`, open
    /// with `self`, `mut self`, `&self`, `&mut self` or `&'a self`, typed or
    /// not.
    fn takes_self(&self, after: usize, stop: usize) -> bool {
        let Some((_, past_name)) = self.word(self.past_blank(after, stop)) else {
            return false;
        };
        let open = self.past_generics(self.past_blank(past_name, stop), stop);
        let open = self.past_blank(open, stop);
        if self.byte(open) != Some(b'(') {
            return false;
        }
        let mut at = self.past_blank(open + 1, stop);
        if self.byte(at) == Some(b'&') {
            at = self.past_blank(at + 1, stop);
            if self.byte(at) == Some(b'\'') {
                let lifetime = self.word(at + 1).map_or(at + 1, |(_, end)| end);
                at = self.past_blank(lifetime, stop);
            }
        }
        if let Some(("mut", end)) = self.word(at) {
            at = self.past_blank(end, stop);
        }
        matches!(self.word(at), Some(("self", _)))
    }

    /// Past the `<…>` that opens at `at`, when one does.
    fn past_generics(&self, at: usize, stop: usize) -> usize {
        if self.byte(at) != Some(b'<') {
            return at;
        }
        let mut depth = 0usize;
        for index in at..stop {
            match self.byte(index) {
                Some(b'<') => depth += 1,
                Some(b'>') if self.closes_generics(index) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return index + 1;
                    }
                }
                _ => {}
            }
        }
        stop
    }

    /// Whether the `>` at `at` closes a `<…>`: the `>` of a `->` closes
    /// nothing.
    fn closes_generics(&self, at: usize) -> bool {
        at.checked_sub(1).and_then(|before| self.byte(before)) != Some(b'-')
    }

    /// Where the item whose words start at `from` stops, what its body holds
    /// and where it ends, before `limit`: at the first `;` at depth 0 of
    /// `()[]{}`, or, where `blocks`, at the `}` that closes the first `{` at
    /// depth 0 that no `<…>` holds, with a `;` that stands right after it.
    /// A `{` within a `<…>` opens a const argument, not a body.
    fn bounds(&self, from: usize, limit: usize, blocks: bool) -> Bounds {
        let mut depth = 0usize;
        let mut generics = 0usize;
        for at in from..limit {
            match self.byte(at) {
                Some(b';') if depth == 0 => {
                    return Bounds {
                        stop: at,
                        body: None,
                        end: at + 1,
                    }
                }
                Some(b'<') if depth == 0 => generics += 1,
                Some(b'>') if depth == 0 && self.closes_generics(at) => {
                    generics = generics.saturating_sub(1);
                }
                Some(b'{') if depth == 0 && generics == 0 && blocks => {
                    let close = self.closing(at, limit).unwrap_or(limit);
                    let after = (close + 1).min(limit);
                    let semicolon = after < limit && self.byte(after) == Some(b';');
                    return Bounds {
                        stop: at,
                        body: Some(at + 1..close),
                        end: after + usize::from(semicolon),
                    };
                }
                Some(b'(' | b'[' | b'{') => depth += 1,
                Some(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        Bounds {
            stop: limit,
            body: None,
            end: limit,
        }
    }

    /// Just past the attribute whose `#` stands at `at`, `#[…]` or `#![…]`;
    /// `None` where no attribute stands there.
    fn past_attribute(&self, at: usize, limit: usize) -> Option<usize> {
        if self.byte(at) != Some(b'#') {
            return None;
        }
        let mut open = self.past_blank(at + 1, limit);
        if self.byte(open) == Some(b'!') {
            open = self.past_blank(open + 1, limit);
        }
        if self.byte(open) != Some(b'[') {
            return None;
        }
        Some(self.closing(open, limit).map_or(limit, |close| close + 1))
    }

    /// Whether the attribute whose `#` stands at `at` is an inner one,
    /// `#![…]`, which belongs to what holds it.
    fn inner(&self, at: usize, limit: usize) -> bool {
        self.byte(self.past_blank(at + 1, limit)) == Some(b'!')
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

    /// The word of code that starts at `at`, and where it ends: a run of
    /// letters, digits and `_`, a raw identifier `r#…` among them.
    fn word(&self, at: usize) -> Option<(&str, usize)> {
        let raw = self.code.get(at..at + 2) == Some(b"r#".as_slice())
            && self.byte(at + 2).is_some_and(is_word);
        let from = at + 2 * usize::from(raw);
        let rest = self.code.get(from..).unwrap_or_default();
        let end = from + rest.iter().take_while(|&&byte| is_word(byte)).count();
        if end == from {
            return None;
        }
        Some((std::str::from_utf8(self.code.get(at..end)?).ok()?, end))
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

    fn byte(&self, at: usize) -> Option<u8> {
        self.code.get(at).copied()
    }
}

//! How Rust declares its items: by the word that declares each, past its
//! attributes and its qualifiers, through the `;` that ends it or the
//! braces of its body.

use super::{is_word, named, Bounds, Item, Kind, Text};

/// The kind `word` declares, when it declares one.
fn kind_of(word: &str) -> Option<Kind> {
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

/// Whether a body ends an item of `kind`: a `const`, a `static` and a `type`
/// end at their `;`, whatever block their value holds.
fn ends_at_its_body(kind: Kind) -> bool {
    !matches!(kind, Kind::Const | Kind::Static | Kind::Type)
}

impl Text<'_> {
    /// The item that stands at `at` or past the blank there, before
    /// `limit`, and where the next may start. What declares no item a shape
    /// names is read past with none: a stray `;`, an inner attribute, a
    /// `use`, an `extern crate`, a `mod x;`, a macro's call.
    pub(super) fn rust_item(&self, at: usize, limit: usize) -> (Option<Item>, usize) {
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
    /// next may start: `None` for one of no kind a shape declares, or one
    /// whose keyword stands at or past `limit`.
    fn declared(&self, start: usize, limit: usize) -> (Option<Item>, usize) {
        let mut head = start;
        while let Some(past) = self.past_attribute(head, limit) {
            head = self.past_blank(past, limit);
        }
        let at = self.qualifiers(head, limit);
        let keyword = self
            .word(at)
            .filter(|_| at < limit)
            .and_then(|(word, after)| Some((kind_of(word)?, after)));
        let blocks = keyword.is_none_or(|(kind, _)| ends_at_its_body(kind));
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
            lead: None,
        };
        (Some(item), next)
    }

    /// Past the qualifiers that stand at `at`: `pub` and its `(…)`,
    /// `async`, `unsafe`, `extern` and the ABI after it, `default`, and a
    /// `const` a function's words follow.
    fn qualifiers(&self, mut at: usize, limit: usize) -> usize {
        while at < limit {
            let Some((word, after)) = self.word(at) else {
                break;
            };
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
    /// `None` where no attribute stands there, or at or past `limit`.
    fn past_attribute(&self, at: usize, limit: usize) -> Option<usize> {
        if at >= limit || self.byte(at) != Some(b'#') {
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

    /// The word of code that starts at `at`, and where it ends: a run of
    /// letters, digits, `_` and bytes of characters past ASCII, a raw
    /// identifier `r#…` among them.
    fn word(&self, at: usize) -> Option<(&str, usize)> {
        let raw = self.code.get(at..at + 2) == Some(b"r#".as_slice())
            && self.byte(at + 2).is_some_and(named);
        let from = at + 2 * usize::from(raw);
        let rest = self.code.get(from..).unwrap_or_default();
        let end = from + rest.iter().take_while(|&&byte| named(byte)).count();
        if end == from {
            return None;
        }
        Some((std::str::from_utf8(self.code.get(at..end)?).ok()?, end))
    }
}

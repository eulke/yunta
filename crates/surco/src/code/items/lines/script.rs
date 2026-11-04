//! How a script, JavaScript or TypeScript, declares its items: at its top,
//! past `export` and its kin, a `function`, a `class`, an `interface`, a
//! `type`, an `enum`, a `namespace` or a name bound to a value; inside a
//! class or an interface, its methods.

use super::super::{is_word, Kind, Text};
use super::Declared;
use crate::code::listed;

/// The words that only qualify a script's declaration at its top.
const SCRIPT_QUALIFIERS: &str = "export default declare async abstract";

/// The words that only qualify a member of a script's class or interface.
const MEMBER_MODIFIERS: &str =
    "public private protected static readonly async abstract override declare get set accessor";

impl Text<'_> {
    /// What a script's statement declares, past `export`, `default`,
    /// `declare`, `async` and `abstract`: a `function`, a `class`, an
    /// `interface`, a `type`, an `enum`, a `namespace` or a `module`; or the
    /// name a `const`, a `let` or a `var` binds, a function where its value
    /// is one.
    pub(super) fn script(&self, head: usize, end: usize) -> Option<Declared> {
        let (word, after) = self.declaring_word(head, end)?;
        let at = self.past_blank(after, end);
        let at = match (word.as_str(), self.byte(at)) {
            ("function", Some(b'*')) => self.past_blank(at + 1, end),
            _ => at,
        };
        let (name, past) = self.word_at(at)?;
        let kind = match word.as_str() {
            "function" => Kind::Fn,
            "const" | "let" | "var" if self.binds_function(past, end) => Kind::Fn,
            "const" => Kind::Const,
            "let" | "var" => Kind::Var,
            "class" => Kind::Class,
            "interface" => Kind::Interface,
            "type" => Kind::Type,
            "enum" => Kind::Enum,
            "namespace" | "module" => Kind::Mod,
            _ => return None,
        };
        Some(Declared::One {
            kind,
            name,
            receiver: false,
        })
    }

    /// The word that declares a script's statement from `head`, past the
    /// words that only qualify it, and where that word ends. A `const` an
    /// `enum` follows only qualifies it.
    fn declaring_word(&self, head: usize, end: usize) -> Option<(String, usize)> {
        let mut at = head;
        loop {
            let (word, after) = self.word_at(at)?;
            let next = self.past_blank(after, end);
            let enum_follows =
                word == "const" && self.word_at(next).is_some_and(|(next, _)| next == "enum");
            if !listed(SCRIPT_QUALIFIERS, &word) && !enum_follows {
                return Some((word, after));
            }
            at = next;
        }
    }

    /// Whether the binding whose name ends at `past` holds a function: after
    /// the `=` that binds it, its value is a `function`, or an arrow, `=>`,
    /// stands outside brackets.
    fn binds_function(&self, past: usize, end: usize) -> bool {
        let Some(equals) = self.assignment(past, end) else {
            return false;
        };
        let value = self.past_blank(equals + 1, end);
        let value = match self.word_at(value) {
            Some((word, after)) if word == "async" => self.past_blank(after, end),
            _ => value,
        };
        self.word_at(value)
            .is_some_and(|(word, _)| word == "function")
            || self.arrow_after(value, end).is_some()
    }

    /// Where the `=>` of the arrow function that a script's binding from
    /// `head` to `stop` holds stands: the first after the `=` that binds it.
    pub(super) fn arrow(&self, head: usize, stop: usize) -> Option<usize> {
        let equals = self.assignment(head, stop)?;
        self.arrow_after(equals + 1, stop)
    }

    /// Where the first `=` outside brackets from `from` to `to` assigns, as
    /// against a `==`, a `=>`, a `<=`, a `>=` or a `!=`.
    fn assignment(&self, from: usize, to: usize) -> Option<usize> {
        self.outside_brackets(from, to, |at| {
            let before = at.checked_sub(1).and_then(|before| self.byte(before));
            self.byte(at) == Some(b'=')
                && !matches!(self.byte(at + 1), Some(b'=' | b'>'))
                && !matches!(before, Some(b'=' | b'!' | b'<' | b'>'))
        })
    }

    /// Where the first `=>` outside brackets from `from` to `to` stands.
    fn arrow_after(&self, from: usize, to: usize) -> Option<usize> {
        self.outside_brackets(from, to, |at| {
            self.code.get(at..at + 2) == Some(b"=>".as_slice())
        })
    }

    /// What a member of a script's class or interface declares: a method,
    /// its name followed by its parameters or its type parameters, or a
    /// field whose value is a function, past modifiers such as `static`,
    /// `private`, `async` or `get`. A field that holds no function declares
    /// none.
    pub(super) fn method(&self, head: usize, end: usize) -> Option<Declared> {
        let mut at = head;
        loop {
            if self.byte(at) == Some(b'*') {
                at = self.past_blank(at + 1, end);
                continue;
            }
            let (word, after) = self.word_at(at)?;
            let next = self.past_blank(after, end);
            let modifies = listed(MEMBER_MODIFIERS, &word)
                && self
                    .byte(next)
                    .is_some_and(|byte| byte == b'*' || is_word(byte));
            if modifies {
                at = next;
                continue;
            }
            let optional = match self.byte(next) {
                Some(b'?' | b'!') => self.past_blank(next + 1, end),
                _ => next,
            };
            let called = matches!(self.byte(optional), Some(b'(' | b'<'));
            return (called || self.binds_function(after, end)).then_some(Declared::One {
                kind: Kind::Fn,
                name: word,
                receiver: false,
            });
        }
    }
}

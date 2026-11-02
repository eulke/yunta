//! How Go declares its items: `func`, a method named by its receiver's type
//! and its name, and `type`, `const` and `var`, alone or as a group.

use super::super::{Kind, Text};
use super::Declared;

impl Text<'_> {
    /// What a Go statement declares: a `func`, by its name or, for a method,
    /// by its receiver's type and its name; a `type`, a `const` or a `var`,
    /// alone or as a group.
    pub(super) fn go(&self, head: usize, end: usize) -> Option<Declared> {
        let (word, after) = self.word_at(head)?;
        let kind = match word.as_str() {
            "func" => return self.go_func(after, end),
            "type" => Kind::Type,
            "const" => Kind::Const,
            "var" => Kind::Var,
            _ => return None,
        };
        let at = self.past_blank(after, end);
        if self.byte(at) == Some(b'(') {
            let close = self.closing(at, end)?;
            return Some(Declared::Group {
                kind,
                open: at,
                close,
            });
        }
        let (name, _) = self.word_at(at)?;
        Some(Declared::One {
            kind,
            name,
            receiver: false,
        })
    }

    /// What the Go `func` whose keyword ends at `after` declares: a function
    /// called by its name, or a method by its receiver's type and its name,
    /// as `Store.Load`.
    fn go_func(&self, after: usize, end: usize) -> Option<Declared> {
        let at = self.past_blank(after, end);
        let (owner, at) = match self.byte(at) {
            Some(b'(') => {
                let close = self.closing(at, end)?;
                let owner = self.receiver_type(at + 1, close)?;
                (Some(owner), self.past_blank(close + 1, end))
            }
            _ => (None, at),
        };
        let (name, _) = self.word_at(at)?;
        let name = match owner {
            Some(owner) => format!("{owner}.{name}"),
            None => name,
        };
        Some(Declared::One {
            kind: Kind::Fn,
            name,
            receiver: false,
        })
    }

    /// The type a Go receiver from `from` to `to` names: its last word
    /// before its type arguments, past a `*`.
    fn receiver_type(&self, from: usize, to: usize) -> Option<String> {
        let to = (from..to)
            .find(|&at| self.byte(at) == Some(b'['))
            .unwrap_or(to);
        self.words(from, to)
            .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .find(|word| !word.is_empty())
            .map(str::to_string)
    }
}

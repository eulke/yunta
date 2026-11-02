//! How Python declares its items: a `def`, `async` or not, and a `class`.

use super::super::{Kind, Text};
use super::Declared;

impl Text<'_> {
    /// What a Python statement declares: a `def`, `async` or not, which
    /// takes a receiver where its first parameter is `self` or `cls`; or a
    /// `class`.
    pub(super) fn python(&self, head: usize, end: usize) -> Option<Declared> {
        let (word, after) = self.word_at(head)?;
        let (word, after) = match word.as_str() {
            "async" => self.word_at(self.past_blank(after, end))?,
            _ => (word, after),
        };
        let (name, past) = self.word_at(self.past_blank(after, end))?;
        match word.as_str() {
            "def" => {
                let open = self.past_blank(past, end);
                let open = match self.byte(open) {
                    Some(b'[') => self.past_blank(self.closing(open, end)? + 1, end),
                    _ => open,
                };
                let first = self.word_at(self.past_blank(open + 1, end));
                let receiver = self.byte(open) == Some(b'(')
                    && first.is_some_and(|(first, _)| first == "self" || first == "cls");
                Some(Declared::One {
                    kind: Kind::Fn,
                    name,
                    receiver,
                })
            }
            "class" => Some(Declared::One {
                kind: Kind::Class,
                name,
                receiver: false,
            }),
            _ => None,
        }
    }
}

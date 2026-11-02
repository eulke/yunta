//! How Go, JavaScript or TypeScript, and Python declare their items. A
//! statement starts on a line, and runs on while a bracket it opened is
//! open and over the lines indented further than its first, as gofmt,
//! prettier and black lay code out: so a statement that Go or Python ends
//! with no `;`, or that a script leaves without one, never swallows the
//! next. An item is a statement whose words declare one, with its
//! decorators above it or before it on its line. Each language's words are
//! read in a module of its own.

mod go;
mod python;
mod script;

use super::{named, Bounds, Item, Kind, Text, BYTE_ORDER_MARK};
use crate::code::Language;

/// What a statement's words declare.
enum Declared {
    /// One item, of `kind`, called `name`.
    One {
        kind: Kind,
        name: String,
        receiver: bool,
    },
    /// A Go group, `const (`, `var (` or `type (`, whose lines each declare
    /// an item of `kind`, between its brackets at `open` and `close`.
    Group {
        kind: Kind,
        open: usize,
        close: usize,
    },
}

impl Text<'_> {
    /// The items the statement at `at`, or past the blank there, declares
    /// before `limit`, and where the next statement may start. `holder` is
    /// the kind of the item whose body is read, where one is: a script's
    /// class and interface declare methods without `function`, and each
    /// line of a Go group declares an item of the group's kind.
    pub(super) fn line_items(
        &self,
        at: usize,
        limit: usize,
        holder: Option<Kind>,
    ) -> (Vec<Item>, usize) {
        let start = self.past_blank(at, limit);
        if start >= limit {
            return (Vec::new(), limit);
        }
        let mut head = start;
        let mut end = self.statement_end(head, limit);
        // A decorator stands above what it decorates, or before it on its
        // line.
        while self.byte(head) == Some(b'@') {
            let after = self.decorator_end(head, end);
            if after < self.line_end(after, end) {
                head = after;
                continue;
            }
            head = self.past_blank(end, limit);
            if head >= limit {
                return (Vec::new(), limit);
            }
            end = self.statement_end(head, limit);
        }
        let next = (end + 1).min(limit);
        (self.declared_by(start, head, end, holder), next)
    }

    /// Just past the decorator whose `@` stands at `at`, and the spaces, tabs
    /// and carriage returns after it, before `end`: its dotted name, and its
    /// arguments where it takes some.
    fn decorator_end(&self, at: usize, end: usize) -> usize {
        let mut past = at + 1;
        while let Some((_, after)) = self.word_at(past) {
            past = after;
            if self.byte(past) != Some(b'.') {
                break;
            }
            past += 1;
        }
        if self.byte(past) == Some(b'(') {
            past = self.closing(past, end).map_or(end, |close| close + 1);
        }
        self.past_spaces(past, end)
    }

    /// The items the statement whose words start at `head` and whose last
    /// line ends at `end` declares, its decorators starting at `start`.
    fn declared_by(
        &self,
        start: usize,
        head: usize,
        end: usize,
        holder: Option<Kind>,
    ) -> Vec<Item> {
        let declared = match (self.language, holder) {
            (Language::Go, Some(kind @ (Kind::Const | Kind::Var | Kind::Type))) => {
                self.word_at(head).map(|(name, _)| Declared::One {
                    kind,
                    name,
                    receiver: false,
                })
            }
            (Language::Go, _) => self.go(head, end),
            (Language::Script, Some(Kind::Class | Kind::Interface)) => self.method(head, end),
            (Language::Script, _) => self.script(head, end),
            _ => self.python(head, end),
        };
        match declared {
            Some(Declared::One {
                kind,
                name,
                receiver,
            }) => vec![Item {
                kind,
                name,
                start,
                head,
                bounds: self.line_bounds(kind, head, end),
                receiver,
                lead: None,
            }],
            Some(Declared::Group { kind, open, close }) => {
                // A group writes its keyword once for every item it declares.
                let lead = head..self.word_at(head).map_or(head, |(_, after)| after);
                self.read(open + 1..close, Some(kind))
                    .into_iter()
                    .map(|item| Item {
                        lead: Some(lead.clone()),
                        ..item
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// Where the item of `kind` whose words start at `head` and whose last
    /// line ends at `end` stops, what its body holds and where it ends. A
    /// Python head stops at its `:`, and its body is what follows. Elsewhere
    /// a body is the braces that close the statement, for a function, a
    /// class, an interface, an enum, a namespace and a Go `type`, or the
    /// expression after a script's arrow; any other item stops at its `;`,
    /// or past its last word.
    fn line_bounds(&self, kind: Kind, head: usize, end: usize) -> Bounds {
        let end = self.trimmed(head, end);
        if self.language == Language::Python {
            return self.colon_bounds(head, end);
        }
        let blocks = match self.language {
            Language::Go => matches!(kind, Kind::Fn | Kind::Type),
            _ => !matches!(kind, Kind::Const | Kind::Var | Kind::Type),
        };
        if let Some((open, close)) = self.closing_block(head, end).filter(|_| blocks) {
            return Bounds {
                stop: open,
                body: Some(open + 1..close),
                end,
            };
        }
        let semicolon = end > head && self.byte(end - 1) == Some(b';');
        let stop = end - usize::from(semicolon);
        let arrow = (self.language == Language::Script && kind == Kind::Fn)
            .then(|| self.arrow(head, stop))
            .flatten();
        match arrow {
            Some(arrow) => Bounds {
                stop: arrow,
                body: Some(arrow + 2..stop),
                end,
            },
            None => Bounds {
                stop,
                body: None,
                end,
            },
        }
    }

    /// The braces that close the statement from `head` to `end`: the `}`
    /// its code ends with, a `;` after it or not, and the `{` outside every
    /// other bracket that this `}` closes.
    fn closing_block(&self, head: usize, end: usize) -> Option<(usize, usize)> {
        let last = match end.checked_sub(1).and_then(|at| self.byte(at)) {
            Some(b';') => self.trimmed(head, end - 1),
            _ => end,
        };
        let close = last
            .checked_sub(1)
            .filter(|&close| close >= head && self.byte(close) == Some(b'}'))?;
        let mut depth = 0usize;
        let mut open = None;
        for at in head..close {
            match self.byte(at) {
                Some(b'{') => {
                    if depth == 0 {
                        open = Some(at);
                    }
                    depth += 1;
                }
                Some(b'(' | b'[') => depth += 1,
                Some(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        // The last `{` opened outside every bracket is the one the `}` at
        // `close` closes, when it alone stands open.
        open.filter(|_| depth == 1).map(|open| (open, close))
    }

    /// Where a Python head stops, at the first `:` outside its brackets, and
    /// its body, what follows that `:` through the statement's end, where
    /// anything does.
    fn colon_bounds(&self, head: usize, end: usize) -> Bounds {
        match self.outside_brackets(head, end, |at| self.byte(at) == Some(b':')) {
            Some(stop) => Bounds {
                stop,
                body: (stop + 1 < end).then_some(stop + 1..end),
                end,
            },
            None => Bounds {
                stop: end,
                body: None,
                end,
            },
        }
    }

    /// The first offset from `from` to `to` that `found` holds for, outside
    /// every bracket opened after `from`.
    fn outside_brackets(
        &self,
        from: usize,
        to: usize,
        found: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let mut depth = 0usize;
        for at in from..to {
            match self.byte(at) {
                Some(b'(' | b'[' | b'{') => depth += 1,
                Some(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
                _ if depth == 0 && found(at) => return Some(at),
                _ => {}
            }
        }
        None
    }

    /// The word of code that starts at `at`, and where it ends: a run of
    /// letters, digits, `_` and bytes of characters past ASCII, and in a
    /// script `$` too.
    fn word_at(&self, at: usize) -> Option<(String, usize)> {
        let script = self.language == Language::Script;
        let length = self
            .code
            .get(at..)
            .unwrap_or_default()
            .iter()
            .take_while(|&&byte| named(byte) || (script && byte == b'$'))
            .count();
        let word = self
            .code
            .get(at..at + length)
            .filter(|word| !word.is_empty())?;
        Some((String::from_utf8_lossy(word).into_owned(), at + length))
    }

    /// Where the statement that starts at `start` ends, before `limit`: at
    /// the end of its last line. It runs on while a bracket it opened is
    /// open, and over each line indented further than its first; blank
    /// lines between those are its own, and those after it are not.
    fn statement_end(&self, start: usize, limit: usize) -> usize {
        let base = start - self.line_start(start);
        let mut end = self.line_end(start, limit);
        let mut depth = self.brackets(start, end, 0);
        let mut line = end + 1;
        while line < limit {
            let close = self.line_end(line, limit);
            let first = self.past_spaces(line, close);
            if first < close {
                if depth == 0 && first - line <= base {
                    break;
                }
                depth = self.brackets(first, close, depth);
                end = close;
            }
            line = close + 1;
        }
        end
    }

    /// Where the line holding `at` starts. The first line starts past the
    /// byte-order mark that opens the text, which is read as blank and is no
    /// indent: `at` itself where it stands within the mark.
    fn line_start(&self, at: usize) -> usize {
        let before = self.code.get(..at).unwrap_or_default();
        match before.iter().rposition(|&byte| byte == b'\n') {
            Some(newline) => newline + 1,
            None if self.original.starts_with(BYTE_ORDER_MARK) => BYTE_ORDER_MARK.len().min(at),
            None => 0,
        }
    }

    /// Where the line holding `at` ends, at its line break, or at `limit`.
    fn line_end(&self, at: usize, limit: usize) -> usize {
        self.code
            .get(at..limit)
            .unwrap_or_default()
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(limit, |found| at + found)
    }

    /// Past the spaces, tabs and carriage returns that stand at `at`, before
    /// `to`.
    fn past_spaces(&self, at: usize, to: usize) -> usize {
        let blank = self.code.get(at..to).unwrap_or_default();
        at + blank
            .iter()
            .take_while(|&&byte| matches!(byte, b' ' | b'\t' | b'\r'))
            .count()
    }

    /// `depth` once the brackets of the code from `from` to `to` have opened
    /// and closed.
    fn brackets(&self, from: usize, to: usize, depth: usize) -> usize {
        self.code
            .get(from..to)
            .unwrap_or_default()
            .iter()
            .fold(depth, |depth, byte| match byte {
                b'(' | b'[' | b'{' => depth + 1,
                b')' | b']' | b'}' => depth.saturating_sub(1),
                _ => depth,
            })
    }
}

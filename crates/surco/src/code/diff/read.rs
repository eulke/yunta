//! A code as the word diff reads it: its text, and where each of its words
//! stands in it, so a change is said in the words it changes and what
//! stands between them.

use std::ops::Range;

use crate::code::lex::{self, Language};

/// A code's text, and where each of its words stands in it.
pub(super) struct Read<'a> {
    pub(super) text: &'a str,
    language: Option<Language>,
    pub(super) words: Vec<Range<usize>>,
}

impl<'a> Read<'a> {
    pub(super) fn of(text: &'a str, language: Option<Language>) -> Self {
        Read {
            text,
            language,
            words: lex::words(text, language),
        }
    }

    /// What word `at` says.
    pub(super) fn word(&self, at: usize) -> &'a str {
        &self.text[self.words[at].clone()]
    }

    /// What each word says, in order.
    pub(super) fn said(&self) -> impl Iterator<Item = &'a str> + '_ {
        (0..self.words.len()).map(|at| self.word(at))
    }

    /// What stands between word `at` and the next, its comments left out.
    pub(super) fn gap(&self, at: usize) -> String {
        let between = self.words[at].end..self.words[at + 1].start;
        lex::uncommented(&self.text[between], self.language)
    }

    /// Words `range` as a reader is told them: each with what stands between
    /// it and the next, comments left out and every stretch of whitespace
    /// one space.
    pub(super) fn spoken(&self, range: &Range<usize>) -> String {
        let mut said = String::new();
        for at in range.clone() {
            said.push_str(self.word(at));
            if at + 1 < range.end {
                said.push_str(&self.gap(at));
            }
        }
        collapsed(&said)
    }

    /// Where in the text words `range` stand, from the first's start to the
    /// last's end; for no words, the place right after the word before them.
    pub(super) fn place(&self, range: &Range<usize>) -> Range<usize> {
        if range.is_empty() {
            let at = self.after(range.start);
            return at..at;
        }
        self.words[range.start].start..self.words[range.end - 1].end
    }

    /// The place right after word `at - 1`: before the first word when `at`
    /// is the first, and the text's end when it has no words.
    pub(super) fn after(&self, at: usize) -> usize {
        match at.checked_sub(1).and_then(|before| self.words.get(before)) {
            Some(before) => before.end,
            None => self
                .words
                .first()
                .map_or(self.text.len(), |first| first.start),
        }
    }

    /// Where words `range` stand, a stretch to a line: from a word to the
    /// last of them on its line, with what stands between them, and a word
    /// that runs on past its line, as a string may, cut where each line
    /// ends.
    pub(super) fn on_lines(&self, range: &Range<usize>) -> Vec<Range<usize>> {
        let mut stretches: Vec<Range<usize>> = Vec::new();
        for word in &self.words[range.clone()] {
            match stretches.last_mut() {
                Some(stretch) if !self.text[stretch.end..word.start].contains('\n') => {
                    stretch.end = word.end;
                }
                _ => stretches.push(word.clone()),
            }
        }
        stretches
            .iter()
            .flat_map(|stretch| {
                let mut from = stretch.start;
                self.text[stretch.clone()].split('\n').map(move |piece| {
                    let line = from..from + piece.len();
                    from = line.end + 1;
                    line
                })
            })
            .filter(|line| !line.is_empty())
            .collect()
    }
}

/// `text` with every stretch of whitespace in it one space.
pub(super) fn collapsed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            blank = true;
            continue;
        }
        if blank {
            out.push(' ');
            blank = false;
        }
        out.push(ch);
    }
    if blank {
        out.push(' ');
    }
    out
}

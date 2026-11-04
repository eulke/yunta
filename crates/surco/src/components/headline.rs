//! What a page is about, on its first line.

use crate::glyphs::Mark;

/// What a page is about, on its first line: the mark that says who moves
/// it, the subject, the word it stands at, the facts beside it, and what
/// names it at the line's far edge.
pub struct Headline {
    pub mark: Mark,
    /// What the page is about: a run's workflow and mode, a node, a check.
    pub subject: String,
    /// The word it stands at, with what qualifies it: `needs you`,
    /// `finished in 7h41m`.
    pub said: String,
    /// Facts beside the word, each after a separator; a narrow line drops
    /// them from the last.
    pub facts: Vec<String>,
    /// What names it at the line's far edge: a run's handle, which a
    /// reader copies into the next command.
    pub edge: Option<String>,
}

impl Headline {
    pub fn new(mark: Mark, subject: impl Into<String>, said: impl Into<String>) -> Self {
        Headline {
            mark,
            subject: subject.into(),
            said: said.into(),
            facts: Vec::new(),
            edge: None,
        }
    }

    /// This headline with `fact` beside its word; a fact with nothing in
    /// it says nothing.
    pub fn fact(mut self, fact: impl Into<String>) -> Self {
        let fact = fact.into();
        if !fact.trim().is_empty() {
            self.facts.push(fact);
        }
        self
    }

    /// This headline with what a list of checks under it found beside its
    /// word: how many problems, then how many things are worth a look, each
    /// only when there is one.
    pub fn tally(self, problems: usize, cautions: usize) -> Self {
        let headline = match problems {
            0 => self,
            problems => self.fact(crate::text::counted(problems, "problem")),
        };
        match cautions {
            0 => headline,
            cautions => headline.fact(format!(
                "{} worth a look",
                crate::text::counted(cautions, "thing")
            )),
        }
    }

    /// This headline with `edge` at the line's far edge.
    pub fn edge(mut self, edge: impl Into<String>) -> Self {
        self.edge = Some(edge.into());
        self
    }
}

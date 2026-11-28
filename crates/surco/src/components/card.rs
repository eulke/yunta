//! A decision held in place at a terminal, and the list it is answered from.

use super::Legend;

/// A question a person answers here and now, with keys.
pub struct Card {
    /// What is asked.
    pub question: String,
    /// What the card stands on, at its far edge: the node and the run.
    pub edge: Option<String>,
    /// What a reader needs to answer, each a line under the question.
    pub about: Vec<String>,
    pub body: CardBody,
    /// The keys it answers to, each with what it does: `enter answer`.
    pub legend: Legend,
}

/// What a card holds: its options, or its options with the words the one pointed at is said
/// with.
pub enum CardBody {
    Choosing {
        choices: Choices,
        /// What follows the option pointed at, when something does.
        then: Option<String>,
    },
    /// Words being typed for the option pointed at, every option still in sight over them.
    Saying {
        /// Every option, the pointer on the one the words are said with.
        choices: Choices,
        /// What is asked of them.
        asks: String,
        typed: TypedRow,
        /// Why the words given were not taken.
        refused: Option<String>,
    },
    /// A round of questions, in order, one of them answered at a time: a list open on it
    /// takes the lines the other rows leave, which only one list can.
    Round(Vec<RoundRow>),
}

/// A list with one row pointed at.
pub struct Choices {
    pub rows: Vec<ChoiceRow>,
    pub at: usize,
}

/// One option of a list: what it is picked by, what it does and what choosing it
/// costs, where those are said, and the key the screen under the list takes it by, where
/// one does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceRow {
    pub name: String,
    pub said: Option<String>,
    pub cost: Option<String>,
    /// The key the screen under the list does the same by: a palette's act, beside what it
    /// does and before the command that does it at a shell. A list whose rows carry keys
    /// lays its rows out in those three columns where its width holds them.
    pub key: Option<String>,
}

impl Choices {
    /// Whether any of its rows carries a key, and so the list stands in three columns.
    pub fn keyed(&self) -> bool {
        self.rows.iter().any(|row| row.key.is_some())
    }
}

/// The row words are being typed on: the one line asked of a person, wherever it is
/// asked. Not a list narrowed by typing, whose filter stands on a row of the list's own.
pub struct TypedRow {
    /// What opens the row where it is written on one line, as Markdown and a page write
    /// it: `> `. A terminal draws the row in a box instead.
    pub prompt: String,
    /// The words before the cursor, whole: a surface shows what fits, as a
    /// terminal's box shows the part of them around the cursor.
    pub before: String,
    /// The words after the cursor, whole.
    pub after: String,
}

impl TypedRow {
    /// Whether nothing is typed on the row, before the cursor or after it: where what is
    /// asked of the words, or a placeholder, stands in their place.
    pub fn is_empty(&self) -> bool {
        self.before.is_empty() && self.after.is_empty()
    }
}

/// One question of a round.
pub struct RoundRow {
    /// What it asks.
    pub asked: String,
    /// What a reader should know about answering it, when anything: that it is
    /// optional and what is assumed without an answer.
    pub note: Option<String>,
    /// Where it stands in the round.
    pub stands: RoundStanding,
}

/// Where one question of a round stands.
pub enum RoundStanding {
    /// Answered, with the answer as a reader reads it.
    Answered(String),
    /// Being answered, by typing.
    Typing {
        typed: TypedRow,
        /// Why the words given were not taken.
        refused: Vec<String>,
    },
    /// Being answered, from a list. No `choices` where what was typed leaves none.
    Picking {
        choices: Option<Choices>,
        /// What was typed to narrow the list, when anything was.
        filter: Option<String>,
        /// Why what was picked was not taken.
        refused: Vec<String>,
    },
    /// Still to be answered.
    Ahead,
}

#[cfg(test)]
mod tests;

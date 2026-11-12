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
    /// The keys it answers to, each with what it does: `enter chooses`.
    pub legend: Legend,
}

/// What a card holds: its options, or the words one of them is said with.
pub enum CardBody {
    Choosing {
        choices: Choices,
        /// What follows the option pointed at, when something does.
        then: Option<String>,
    },
    Saying {
        /// The option the words are said with.
        chosen: ChoiceRow,
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
/// costs, where those are said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceRow {
    pub name: String,
    pub said: Option<String>,
    pub cost: Option<String>,
}

/// The row words are being typed on.
pub struct TypedRow {
    /// What opens the row: `> `.
    pub prompt: String,
    /// The words before the cursor, whole: a surface shows what fits, as a
    /// terminal's row does with [`TypedRow::fitted`].
    pub before: String,
    /// The words after the cursor, whole.
    pub after: String,
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

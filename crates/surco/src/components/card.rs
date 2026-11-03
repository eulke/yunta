//! A decision held in place at a terminal, and the list it is answered from.

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
    pub keys: Vec<String>,
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
    /// What the row shows of the words before the cursor.
    pub before: String,
    /// What it shows after the cursor.
    pub after: String,
}

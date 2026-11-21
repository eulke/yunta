//! What a run has spent, on one row: what it counted, one bar a node, and the time it took.

/// What a run has spent, on one row: a count with its label, one bar a node, and times each
/// with its label, in the order a reader takes them. A surface says every part; a terminal too
/// narrow for the whole row lets the bars go first, then each time from the last, each part
/// whole, so no figure is cut. Not a [`Furrow`](super::Furrow), which says where each node
/// stands rather than what it spent.
pub struct Meter {
    /// What the run counted, its figure and its label: `967k tokens`.
    pub count: String,
    /// One bar a node, in the order the nodes stand: what each counted, a shape against the
    /// largest among them that the count's figure says in numbers. A node that counted
    /// nothing is the lowest bar, also where none counted anything.
    pub bars: Vec<f64>,
    /// The times the run took, each with its label, in the order they are said.
    pub times: Vec<Time>,
}

/// A time a meter says, with what it went to: `2m34s at work`.
pub struct Time {
    /// The time, as a figure: `2m34s`.
    pub figure: String,
    /// What the time went to: `at work`, `waiting on you`.
    pub label: String,
    /// Whether the time is a person's, which reads in the person's tone.
    pub by_person: bool,
}

#[cfg(test)]
pub(crate) mod tests;

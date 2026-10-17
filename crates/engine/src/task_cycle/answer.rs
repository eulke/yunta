//! What a person answered a session a cycle picks back up: the one
//! thing that changed for it since it stopped.

/// A session a cycle picks back up, and the answer it is told: what
/// changed since it stopped, which is the one reason it is continued
/// rather than started over.
#[derive(Debug, Clone, PartialEq)]
pub struct Continuing {
    pub session: yunta_core::SessionId,
    pub answer: Answer,
}

/// What a person answered a session, the one thing that changed for it.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The answer to the scope it asked for.
    Scope(yunta_core::events::ScopeAnswer),
    /// A person's review of what it handed over.
    Review(Review),
    /// A person's answer to the departure from the plan it declared.
    Deviation(yunta_core::events::DeviationResolvedPayload),
    /// A person's acceptance that tests it wrote are wrong, which it
    /// writes again.
    Respecify(Respecify),
}

/// What a node that wrote the run's spec writes again: each task whose
/// tests a person accepted are wrong, the departures its session
/// declared from them, and what the person said.
#[derive(Debug, Clone, PartialEq)]
pub struct Respecify {
    pub owed: Vec<RespecifiedTask>,
}

/// One task whose tests are written again.
#[derive(Debug, Clone, PartialEq)]
pub struct RespecifiedTask {
    pub task: yunta_core::TaskId,
    pub departures: Vec<yunta_core::events::DeviationDeclaredPayload>,
    pub said: Option<String>,
}

/// A person's review of what a node handed over: the gate that asked,
/// the option that sent the run back to the node, and what they said.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub gate: yunta_core::NodeId,
    pub option: yunta_core::OptionId,
    pub said: String,
}

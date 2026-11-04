//! Which facts this domain records.

use super::payloads::*;

/// A decision a person makes: an escalation waiting on one, the resolution
/// that answered it, the questions a node asked and their answers, and the
/// moment the engine began asking — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum GateEvent {
    /// Boxed: the escalation is the largest gate payload, and boxing it keeps
    /// the event enum the size of the payloads beside it.
    Waiting(Box<GateWaitingPayload>),
    Resolved(GateResolvedPayload),
    QuestionsAsked(QuestionsAskedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
    AskingOpened(AskingOpenedPayload),
}

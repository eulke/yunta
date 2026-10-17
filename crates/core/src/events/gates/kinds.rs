//! Which facts this domain records.

use super::payloads::*;

/// A decision a person makes: an escalation waiting on one, the — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum GateEvent {
    /// Boxed: the escalation is the largest gate payload, and boxing it keeps
    /// the event enum at the size it had before the escalation grew.
    Waiting(Box<GateWaitingPayload>),
    Resolved(GateResolvedPayload),
    QuestionsAsked(QuestionsAskedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
    AskingOpened(AskingOpenedPayload),
}

//! Which facts this domain records.

use super::payloads::*;

/// A decision a person makes: an escalation waiting on one, the — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum GateEvent {
    Waiting(GateWaitingPayload),
    Resolved(GateResolvedPayload),
    QuestionsAsked(QuestionsAskedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
    AskingOpened(AskingOpenedPayload),
}

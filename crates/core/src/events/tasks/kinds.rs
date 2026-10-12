//! Which facts this domain records.

use super::payloads::*;

/// The tasks document, as a run works it: a task registered, a task
/// that moved, a check a task session asked for, and a departure from
/// the plan it declared and a person answered — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Registered(TaskRegisteredPayload),
    StatusChanged(TaskStatusChangedPayload),
    CheckStarted(TaskCheckStartedPayload),
    CheckAnswered(TaskCheckAnsweredPayload),
    DeviationDeclared(DeviationDeclaredPayload),
    DeviationResolved(DeviationResolvedPayload),
}

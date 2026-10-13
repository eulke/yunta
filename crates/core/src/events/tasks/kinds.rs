//! Which facts this domain records.

use super::payloads::*;

/// The tasks document, as a run works it: a task registered, a task
/// that moved, a check a task session asked for, a departure from the
/// plan it declared and a person answered, and a criterion it declared
/// no work of the plan can pass — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Registered(TaskRegisteredPayload),
    StatusChanged(TaskStatusChangedPayload),
    CheckStarted(TaskCheckStartedPayload),
    CheckAnswered(TaskCheckAnsweredPayload),
    DeviationDeclared(DeviationDeclaredPayload),
    DeviationResolved(DeviationResolvedPayload),
    CriterionDeclaredUnpassable(CriterionUnpassablePayload),
}

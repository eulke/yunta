//! See [`super`]. What a card's decision rests on, drawn above it: read from the menu of the
//! escalation the engine rebuilt, and from a failure's cause, so a failure's menu stands on
//! the grounds its cause's own menu would.

use yunta_core::events::Failure;

/// What a card's decision rests on, drawn above it. Not `Evidence`, a criterion's proof;
/// not render's `Gate::AboveTheCard`, the page a gate prints over a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grounds {
    /// What a gate shows, with the change.
    Shown,
    /// A failure's output.
    Failure,
    /// A scope asked for: the write the fence refused, where one was.
    Scope,
    /// What the plan decided, and what a session did instead.
    Departure,
    /// What a cap spent against it.
    Cap,
}

/// What a card on `failure` rests on, read from its cause, so a failure's menu stands on
/// the grounds its cause's own menu would: a loop capped on the cap it spent, a scope owed
/// on the writes asked for, a loop's tasks each stopped on a departure owed on the
/// departure; any other failure on its output.
pub(super) fn rests_on(failure: &Failure) -> Grounds {
    match failure {
        Failure::LoopCapped { .. } => Grounds::Cap,
        Failure::ScopeOwed { .. } => Grounds::Scope,
        Failure::TasksStopped { .. } => match failure.departures_owed() {
            Some(_) => Grounds::Departure,
            None => Grounds::Failure,
        },
        Failure::Artifacts { .. } | Failure::SessionDied { .. } => Grounds::Failure,
        Failure::ScopeViolated { .. } | Failure::ScopeRequested { .. } => Grounds::Failure,
        Failure::Unset { .. } | Failure::Unchanged { .. } => Grounds::Failure,
        Failure::PathsDenied { .. } | Failure::Exited { .. } => Grounds::Failure,
        Failure::PlanUnpassable { .. } | Failure::SessionRefused { .. } => Grounds::Failure,
        Failure::Limit { .. } | Failure::Red { .. } => Grounds::Failure,
        Failure::ChildFailed { .. } | Failure::FrozenInput { .. } => Grounds::Failure,
        Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => Grounds::Failure,
        Failure::TokenBudget { .. } | Failure::Template { .. } => Grounds::Failure,
        Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => Grounds::Failure,
        Failure::MountUnreachable { .. } | Failure::GateChose { .. } => Grounds::Failure,
        Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => Grounds::Failure,
        Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => Grounds::Failure,
        Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => Grounds::Failure,
        Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => Grounds::Failure,
        Failure::Confinement { .. } | Failure::FrozenRead { .. } => Grounds::Failure,
        Failure::ChildrenFailed { .. } | Failure::Uncertain { .. } => Grounds::Failure,
        Failure::Rejected { .. } | Failure::SentBack { .. } => Grounds::Failure,
        Failure::FindingsOpen { .. } | Failure::Interrupted { .. } => Grounds::Failure,
        Failure::ChangesRequested { .. } | Failure::PullRequestClosed { .. } => Grounds::Failure,
        Failure::LoopStalled { .. } | Failure::NoTasksDocument { .. } => Grounds::Failure,
        Failure::RunnerUnresolved { .. } | Failure::Message { .. } => Grounds::Failure,
    }
}

//! Which facts this domain records.

use super::payloads::*;

/// An agent session: the CLI it opened on, what it said while it ran, — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    Opened(AgentSessionOpenedPayload),
    Message(AgentMessagePayload),
    CapabilityDegraded(CapabilityDegradedPayload),
    WriteRefused(WriteRefusedPayload),
    /// The session asked to stop with work its close would not take, and
    /// was sent back to it.
    StopRefused(StopRefusedPayload),
    /// What the session wrote, formatted the moment it wrote it.
    WriteFormatted(WriteFormattedPayload),
    RunToolFailed(RunToolFailedPayload),
    /// A call this binary refused, and why; the session was answered with
    /// the refusal.
    RunToolRefused(RunToolRefusedPayload),
    /// The session lost the service behind it and waits for it.
    ServiceUnreachable(ServiceUnreachablePayload),
    /// The service answers again, and the same session goes on.
    ServiceReachable(ServiceReachablePayload),
}

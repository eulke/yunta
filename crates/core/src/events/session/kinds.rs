//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// An agent session: the CLI it opened on, what it said while it ran, — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    Opened(AgentSessionOpenedPayload),
    Message(AgentMessagePayload),
    CapabilityDegraded(CapabilityDegradedPayload),
    WriteRefused(WriteRefusedPayload),
    RunToolFailed(RunToolFailedPayload),
}

impl SessionEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "agent_session_opened",
        "agent_message",
        "capability_degraded",
        "write_refused",
        "run_tool_failed",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Opened(_) => "agent_session_opened",
            Self::Message(_) => "agent_message",
            Self::CapabilityDegraded(_) => "capability_degraded",
            Self::WriteRefused(_) => "write_refused",
            Self::RunToolFailed(_) => "run_tool_failed",
        }
    }

    /// Whether this kind is audit: the log carries it so a reader can
    /// see what the engine did, and no ledger moves when it arrives.
    ///
    /// Every other kind moves state, and its ledger's `apply` names it.
    /// The pair is proved: `a_kind_that_moves_no_state_says_so_by_name`
    /// applies one of each to an empty state and checks that exactly the
    /// kinds answering `false` here change it.
    pub fn is_audit(&self) -> bool {
        match self {
            Self::Opened(_) => false,
            Self::Message(_) => false,
            Self::CapabilityDegraded(_) => false,
            Self::WriteRefused(_) => false,
            Self::RunToolFailed(_) => false,
        }
    }

    /// The shape version of this kind. Every kind starts at 1 and a
    /// version is per kind, never per domain and never global: a kind
    /// that gains an incompatible shape becomes a new name, and only
    /// that one moves.
    pub fn schema_version(&self) -> u32 {
        1
    }
}

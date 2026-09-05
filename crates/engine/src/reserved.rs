//! The escalation options the engine itself builds and interprets. Their
//! spellings live here once, so no run-loop branch ever matches a human's
//! choice or a node's outcome against a bare string literal — the reserved
//! vocabulary is one closed enum, built from it and read back through it.

use std::str::FromStr;

use yunta_core::OptionId;

/// An option the engine appends to an escalation and acts on when a human
/// picks it — as opposed to an author-declared option, which the engine
/// only records. Everything the run loop decides on goes through this
/// enum, never a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReservedOption {
    /// Pause the run with the decision recorded (every escalation offers it).
    Abort,
    /// Re-route to the corrective node once more, past the reroute cap.
    Retry,
    /// Accept promotion to the next declared mode.
    Promote,
    /// Approve a published gate.
    Approve,
    /// Grant a scope expansion a task requested.
    Grant,
    /// Deny a scope expansion a task requested.
    Deny,
    /// Lift a budget cap for this invocation only.
    Continue,
    /// Reject a published gate resolved from the console.
    Reject,
}

impl ReservedOption {
    /// The YAML/log spelling — the one place each option's text is
    /// written, for both building an escalation and comparing a choice.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ReservedOption::Abort => "abort",
            ReservedOption::Retry => "retry",
            ReservedOption::Promote => "promote",
            ReservedOption::Approve => "approve",
            ReservedOption::Grant => "grant",
            ReservedOption::Deny => "deny",
            ReservedOption::Continue => "continue",
            ReservedOption::Reject => "reject",
        }
    }

    /// The option as an escalation offers it.
    pub(crate) fn id(self) -> OptionId {
        OptionId::from_static(self.as_str())
    }

    /// The reserved option `id` spells, or `None` for an author's own.
    pub(crate) fn of(id: &OptionId) -> Option<Self> {
        id.as_str().parse().ok()
    }
}

impl FromStr for ReservedOption {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "abort" => Ok(ReservedOption::Abort),
            "retry" => Ok(ReservedOption::Retry),
            "promote" => Ok(ReservedOption::Promote),
            "approve" => Ok(ReservedOption::Approve),
            "grant" => Ok(ReservedOption::Grant),
            "deny" => Ok(ReservedOption::Deny),
            "continue" => Ok(ReservedOption::Continue),
            "reject" => Ok(ReservedOption::Reject),
            _ => Err(()),
        }
    }
}

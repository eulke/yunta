//! The policies a node declares about itself: what its session may do,
//! how it recovers from a crash, and what a loop runs until.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NodePermissions {
    ReadOnly,
    Edit,
    Full,
}

impl NodePermissions {
    /// The YAML spelling, for diagnostics and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            NodePermissions::ReadOnly => "read-only",
            NodePermissions::Edit => "edit",
            NodePermissions::Full => "full",
        }
    }
}

/// A node's crash-recovery policy — the full triple of options.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OnInterrupt {
    #[default]
    RestartNode,
    FailIfUncertain,
    /// Continue the same agent conversation — the
    /// `session_id` the log recorded (`agent_session_opened`) is
    /// handed back to the adapter's `resume`. Only `kind: prompt` opens
    /// a node-scoped session, so `check` refuses the explicit
    /// declaration anywhere else; an adapter without the
    /// `resume_session` capability — or a crash before any session
    /// opened — degrades to `restart_node` with an explicit
    /// `capability_degraded` event, never silently. As a *config default*
    /// (`defaults.on_interrupt`) it applies where a session exists;
    /// kinds without one (bash/check/…, and a loop's per-task sessions)
    /// restart, which is the only meaning the policy can have there.
    ResumeSession,
}

impl OnInterrupt {
    /// The YAML spelling, for diagnostics and the log.
    pub fn as_str(self) -> &'static str {
        match self {
            OnInterrupt::RestartNode => "restart_node",
            OnInterrupt::FailIfUncertain => "fail_if_uncertain",
            OnInterrupt::ResumeSession => "resume_session",
        }
    }
}

/// What a `kind: loop` runs until. One condition exists: the tasks document
/// has no task left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopUntil {
    AllTasksComplete,
}

impl LoopUntil {
    /// The YAML spelling, for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            LoopUntil::AllTasksComplete => "all_tasks_complete",
        }
    }
}

impl std::fmt::Display for LoopUntil {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

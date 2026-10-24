//! The policies a node declares about itself: what its session may do,
//! how it recovers from a crash, and what a loop runs until.

use serde::{Deserialize, Serialize};

use super::Node;
use crate::port::PermissionProfile;

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

    /// The session profile this permission asks the adapter for.
    pub fn profile(self) -> PermissionProfile {
        match self {
            NodePermissions::ReadOnly => PermissionProfile::ReadOnly,
            NodePermissions::Edit => PermissionProfile::Edit,
            NodePermissions::Full => PermissionProfile::Full,
        }
    }
}

impl Node {
    /// Whether this node's session may write under the checkout. An absent
    /// `permissions` is `edit`, which writes.
    pub fn writes_worktree(&self) -> bool {
        self.permissions
            .is_none_or(|permissions| permissions.profile().writes_worktree())
    }
}

/// Whose configuration of the agent's CLI a session loads beside what the
/// run mounts for it.
///
/// `inherit` loads the person's own — their settings, extensions, MCP
/// servers and hooks — and adds the run's on top; `isolated` loads only
/// what the run mounts, so a session behaves the same on any machine. The
/// CLI's credentials are the person's either way.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CliConfig {
    #[default]
    Inherit,
    Isolated,
}

/// How long the model reasons before it answers. The four levels every
/// adapter's CLI accepts, each handed to it under the CLI's own name for
/// them; a level one CLI has and another lacks is no level here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
}

impl Effort {
    /// The YAML spelling, which is also what both CLIs call the level.
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
        }
    }
}

/// A node's crash-recovery policy — the full triple of options.
///
/// `restart_node` runs the node again from its start, as a new attempt:
/// the context it is handed is rebuilt from the run's data, so the attempt
/// can always begin, but its effect has to be safe to repeat.
/// `fail_if_uncertain` is for an effect that is not — a notification with
/// no key to dedupe on, a charge: a resume that finds the node running with
/// no terminal pauses for a person instead of guessing how it ended.
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

/// Which rounds of a node's questions stop the run for a person.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Ask {
    /// Every round that asks something waits for its answers.
    #[default]
    Always,
    /// Only a round with a required question waits; a round of optional
    /// ones goes on with what each assumes.
    Required,
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

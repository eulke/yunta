use std::fmt;

use serde::{Deserialize, Serialize};

/// An adapter's declared capabilities. Constant for the
/// lifetime of the adapter — the engine consults this before asking
/// for anything, and never emulates what is not declared.
///
/// Lives in `yunta-core` rather than `yunta-adapters` because it is also
/// the shape of `agent_session_opened`'s `capabilities` field;
/// both the event log and the `Adapter` trait
/// share this one definition instead of duplicating it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(default)]
pub struct Capabilities {
    /// Can resume a previous conversation via `resume()`.
    pub resume_session: bool,
    /// What this adapter can build to keep a session's writes inside
    /// its fence. `None` means nothing: the post-check diff is the only
    /// thing that catches a write outside the scope. A log written
    /// before the fence existed carries `edit_hooks` instead, which this
    /// field's `default` reads as `None`.
    pub fence: FenceLevel,
    /// Distinguishes permission profiles (`read_only`/`edit`/`full`).
    pub permission_profiles: bool,
    /// Supports the CLI's own named agents, selectable via the portable
    /// `agent:` field.
    pub custom_agents: bool,
    /// Emits trustworthy token usage in the stream.
    pub usage_reporting: bool,
    /// Can mount skill directories (`SessionRequest.skills`) by the
    /// CLI's native mechanism. A skill is added instruction,
    /// never correctness: absence degrades with `capability_degraded`,
    /// never a fatal error.
    #[serde(default)]
    pub skills: bool,
    /// Can connect to Yunta's per-run MCP server as a client.
    pub run_tools: bool,
    /// Can confine a session's process to no network access, enforcing a
    /// node's `network: false`. Declarative policy is never an OS sandbox
    /// Yunta core promises (D105): where this is absent, a node's
    /// `network: false` degrades with `capability_degraded` — recorded for
    /// policy and audit, never enforced.
    #[serde(default)]
    pub network_isolation: bool,
}

/// What an adapter can build to keep a session's writes inside its
/// fence. Three levels, because three is what the market has: nothing;
/// a judgement before each file-tool call; or a filesystem sandbox the
/// process itself runs under.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FenceLevel {
    #[default]
    None,
    ToolCalls,
    Filesystem,
}

/// One capability an adapter can declare: the closed set of
/// [`Capabilities`] fields, spelled the way `capability_degraded` records
/// them (`resume_session`, `run_tools`, …). The engine names the
/// capability it consulted with this type, so a degradation can never
/// cite a capability no adapter has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ResumeSession,
    Fence,
    PermissionProfiles,
    CustomAgents,
    UsageReporting,
    Skills,
    RunTools,
    NetworkIsolation,
}

impl Capability {
    /// Every capability an adapter can declare. The closed set, in the
    /// order [`Capabilities`] declares them — what a table about
    /// capabilities is checked against, so one added here and nowhere
    /// else stops compiling or fails its test.
    pub const ALL: [Capability; 8] = [
        Capability::ResumeSession,
        Capability::Fence,
        Capability::PermissionProfiles,
        Capability::CustomAgents,
        Capability::UsageReporting,
        Capability::Skills,
        Capability::RunTools,
        Capability::NetworkIsolation,
    ];

    /// The field name, as the log and the spec spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::ResumeSession => "resume_session",
            Capability::Fence => "fence",
            Capability::PermissionProfiles => "permission_profiles",
            Capability::CustomAgents => "custom_agents",
            Capability::UsageReporting => "usage_reporting",
            Capability::Skills => "skills",
            Capability::RunTools => "run_tools",
            Capability::NetworkIsolation => "network_isolation",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Capabilities {
    /// Whether this adapter declares `capability`. The engine asks here
    /// before relying on anything a capability names, and records a
    /// `capability_degraded` for the same [`Capability`] when the answer
    /// is `false`.
    pub fn declares(&self, capability: Capability) -> bool {
        match capability {
            Capability::ResumeSession => self.resume_session,
            Capability::Fence => self.fence != FenceLevel::None,
            Capability::PermissionProfiles => self.permission_profiles,
            Capability::CustomAgents => self.custom_agents,
            Capability::UsageReporting => self.usage_reporting,
            Capability::Skills => self.skills,
            Capability::RunTools => self.run_tools,
            Capability::NetworkIsolation => self.network_isolation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spelling_is_the_one_serde_writes() {
        for capability in [
            Capability::ResumeSession,
            Capability::Fence,
            Capability::PermissionProfiles,
            Capability::CustomAgents,
            Capability::UsageReporting,
            Capability::Skills,
            Capability::RunTools,
            Capability::NetworkIsolation,
        ] {
            let written = serde_json::to_value(capability).unwrap();
            assert_eq!(written, serde_json::json!(capability.as_str()));
            assert_eq!(capability.to_string(), capability.as_str());
        }
    }

    #[test]
    fn declares_reads_the_field_the_capability_names() {
        let caps = Capabilities {
            run_tools: true,
            ..Capabilities::default()
        };
        assert!(caps.declares(Capability::RunTools));
        assert!(!caps.declares(Capability::Skills));
    }
}

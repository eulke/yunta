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
    /// Can block edits outside a set of globs as they happen (hot
    /// enforcement of scope).
    pub edit_hooks: bool,
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

/// One capability an adapter can declare: the closed set of
/// [`Capabilities`] fields, spelled the way `capability_degraded` records
/// them (`resume_session`, `run_tools`, …). The engine names the
/// capability it consulted with this type, so a degradation can never
/// cite a capability no adapter has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ResumeSession,
    EditHooks,
    PermissionProfiles,
    CustomAgents,
    UsageReporting,
    Skills,
    RunTools,
    NetworkIsolation,
}

impl Capability {
    /// The field name, as the log and the spec spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::ResumeSession => "resume_session",
            Capability::EditHooks => "edit_hooks",
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
            Capability::EditHooks => self.edit_hooks,
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
            Capability::EditHooks,
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

use serde::{Deserialize, Serialize};

/// An adapter's declared capabilities. Constant for the
/// lifetime of the adapter — the engine consults this before asking
/// for anything, and never emulates what is not declared.
///
/// Lives in `yunta-core` rather than `yunta-adapters` because it is also
/// the shape of `agent_session_opened`'s `capabilities` field;
/// both the event log and the `Adapter` trait
/// share this one definition instead of duplicating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
}

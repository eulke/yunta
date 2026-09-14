//! An agent session: the CLI it opened on, what it said while it ran,
//! and a capability the adapter did not have.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fence::Coverage;
use crate::hash::ContentHash;
use crate::ids::{AdapterId, AgentName, ModelName, SessionId};
use crate::{Capabilities, Capability};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
}

impl TokenUsage {
    /// Input plus output — the single figure token cost is measured by,
    /// derived in one place so no two call sites can add it up differently.
    /// `cached` is a subset of `input`, already counted, never added on top.
    pub fn total(&self) -> u64 {
        self.input + self.output
    }
}

/// Field by field; `cached` stays unknown only while nobody reported it.
impl std::ops::Add for TokenUsage {
    type Output = TokenUsage;

    fn add(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input + other.input,
            output: self.output + other.output,
            cached: match (self.cached, other.cached) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            },
        }
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, other: TokenUsage) {
        *self = *self + other;
    }
}

impl std::iter::Sum for TokenUsage {
    fn sum<I: Iterator<Item = TokenUsage>>(iter: I) -> TokenUsage {
        iter.fold(TokenUsage::default(), |total, usage| total + usage)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentSessionOpenedPayload {
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentName>,
    /// The model the CLI reported for the session; absent when it
    /// reported none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    pub capabilities: Capabilities,
    /// How much of this session the adapter's fence covered — derived
    /// from what it built, never declared. Absent when it built none.
    /// The level travels once, in `capabilities.fence`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fence: Option<Coverage>,
}

/// A write the fence refused before it happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WriteRefusedPayload {
    pub session_id: SessionId,
    pub target: ToolTarget,
}

impl WriteRefusedPayload {
    pub fn new(session_id: SessionId, target: ToolTarget) -> Self {
        WriteRefusedPayload { session_id, target }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageType {
    ToolUse,
    Usage,
    Note,
}

/// What a tool call acted on, as the log may carry it.
///
/// A tool's argument is the session's own text: a path, a shell command,
/// a URL. Some of it names the repository, which a reader needs; some of
/// it is whatever the session typed, which may be anything at all,
/// including a secret. So the log carries two things and they are not
/// the same thing: `display` is what a reader may be shown and is only
/// ever present for a value the engine itself can vouch for, and
/// `digest` is what identifies the target — the whole hash, so two
/// targets never share one. `abbreviated()` is how a hash is shown,
/// never how it is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolTarget {
    /// The target as a reader may see it. Absent whenever the value is
    /// the session's own text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// What identifies the target, whether or not it can be shown.
    pub digest: ContentHash,
}

impl ToolTarget {
    /// A path the session acted on: shown as written, which is how a
    /// reader finds the file, and hashed so two edits of one file read
    /// as the same target.
    pub fn of_path(path: &Path) -> Self {
        let display = path.display().to_string();
        ToolTarget {
            digest: crate::sha256_hex(display.as_bytes()),
            display: Some(display),
        }
    }

    /// A target the log identifies and never shows: a command, a URL,
    /// a query — the session's own text, which the engine cannot vouch
    /// for and therefore does not repeat.
    pub fn opaque(input: &[u8]) -> Self {
        ToolTarget {
            display: None,
            digest: crate::sha256_hex(input),
        }
    }

    /// The one line a surface shows for this target: what it names when
    /// it can be named, and the shortened hash when it cannot.
    pub fn sentence(&self) -> String {
        match &self.display {
            Some(display) => display.clone(),
            None => self.digest.abbreviated(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentMessagePayload {
    pub message_type: AgentMessageType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// What the tool acted on. Absent for a message that is not a tool
    /// call, and for a log written before the engine recorded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ToolTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// What the engine does when an adapter does not declare a capability it
/// asked for. A closed set: the engine has one fallback per capability
/// it consults, and a degradation naming anything else would be a
/// fallback nobody implemented.
///
/// The sentence a reader sees is produced here, once, by `Display` — so
/// the same fallback reads the same way wherever it is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// `edit_hooks` absent: the session edits freely and the diff is
    /// judged afterwards.
    PostCheckOnly,
    /// `usage_reporting` absent: the run's token cap cannot be counted
    /// against this session.
    NoTokenBudget,
    /// `skills` absent: the session runs with none mounted.
    NoSkills,
    /// `run_tools` absent, or its server unreachable: the session holds
    /// none of the run's tools.
    NoRunTools,
    /// `network_isolation` absent: `network: false` is recorded, not
    /// enforced.
    NetworkOpen,
    /// `resume_session` absent, or nothing to resume: a fresh session
    /// replaces the interrupted one.
    FreshSession,
}

impl std::fmt::Display for Policy {
    /// What the engine did instead, in the words every surface prints.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Policy::PostCheckOnly => {
                "post-check only — the session edits unguarded and its diff is judged \
                 against the declared scope after the fact"
            }
            Policy::NoTokenBudget => {
                "no token budget — the session reports no usage, so the run's cap is \
                 recorded against it as zero"
            }
            Policy::NoSkills => {
                "skills not mounted — the session runs without the skills the node \
                 declares"
            }
            Policy::NoRunTools => {
                "run tools unreachable — the session holds none of the run's tools, so \
                 this node ends owing every document it declares"
            }
            Policy::NetworkOpen => {
                "declarative only — `network: false` is recorded for policy and audit, \
                 not enforced"
            }
            Policy::FreshSession => "restart_node — a fresh session replaces the interrupted one",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityDegradedPayload {
    /// The capability the engine consulted and the adapter does not declare.
    pub capability: Capability,
    pub adapter: AdapterId,
    policy_applied: String,
}

impl CapabilityDegradedPayload {
    /// A capability the adapter does not have, and what the engine did
    /// instead.
    ///
    /// Both halves are the point: a degradation that names only what was
    /// missing leaves a reader guessing whether anything still holds,
    /// and one that names only the fallback hides that something was
    /// asked for and refused. The engine writes them together or not at
    /// all.
    pub fn new(capability: Capability, adapter: AdapterId, policy: Policy) -> Self {
        CapabilityDegradedPayload {
            capability,
            adapter,
            policy_applied: policy.to_string(),
        }
    }

    /// What the engine did instead.
    pub fn policy_applied(&self) -> &str {
        &self.policy_applied
    }
}

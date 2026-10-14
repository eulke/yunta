//! An agent session: the CLI it opened on, what it said while it ran,
//! and a capability the adapter did not have.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::best_effort::BestEffortStep;
use crate::confinement::{Commands, Confinement, Worktree};
use crate::fence::Coverage;
use crate::hash::ContentHash;
use crate::ids::{AdapterId, AgentName, ModelName, SessionId, TaskId};
use crate::port::{PermissionProfile, Policy, PolicyApplied};
use crate::rail::hook::RailFailureCause;
use crate::rail::RailEvent;
use crate::{Capabilities, Capability};

pub use crate::rail::timing::RailSummaryPayload;

/// The closed reason a run tool call failed, without adapter error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunToolFailureCause {
    ApprovalBlocked,
    CallFailed,
}

impl RunToolFailureCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApprovalBlocked => "approval_blocked",
            Self::CallFailed => "call_failed",
        }
    }
}

/// Why this binary refused a call to one of the tools it serves on
/// `yunta-run`: a closed set, so the log says it without the prose the
/// session was answered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunToolRefusal {
    /// A request of the same kind was already written and waits on a
    /// decision.
    RequestPending,
    /// The call's arguments do not say what the tool needs.
    InvalidArguments,
    /// The tool is not one this session may call here.
    NotOfferedHere,
    /// The finding the call names cannot be answered the way it asks.
    FindingNotAnswerable,
    /// The document handed over was refused, or was already superseded.
    Refused,
    /// The engine could not do what the call asked of it.
    EngineFailed,
}

impl RunToolRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequestPending => "request_pending",
            Self::InvalidArguments => "invalid_arguments",
            Self::NotOfferedHere => "not_offered_here",
            Self::FindingNotAnswerable => "finding_not_answerable",
            Self::Refused => "refused",
            Self::EngineFailed => "engine_failed",
        }
    }
}

/// A call to a tool this binary serves that it refused, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunToolRefusedPayload {
    pub tool: crate::RunTool,
    pub reason: RunToolRefusal,
}

/// A session cut off from the service behind it: it waits for the service
/// to answer, and goes on as the same session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServiceUnreachablePayload {
    pub session_id: SessionId,
    /// What the session's CLI said when it lost the service.
    pub message: String,
}

/// The service a session lost answers again, and the session goes on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServiceReachablePayload {
    /// How long the session waited for it, in the host's awake time.
    pub waited_ms: u64,
}

/// One failed call to a tool this binary serves on `yunta-run`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunToolFailedPayload {
    pub session_id: SessionId,
    pub tool: crate::RunTool,
    pub cause: RunToolFailureCause,
}

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

    /// Field by field, never below zero: usage that settles more than was
    /// in flight leaves nothing behind. `cached` stays unknown only while
    /// nothing reported it, as it does under `+`.
    pub fn saturating_sub(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input.saturating_sub(other.input),
            output: self.output.saturating_sub(other.output),
            cached: self
                .cached
                .map(|cached| cached.saturating_sub(other.cached.unwrap_or(0))),
        }
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

/// What the session was allowed to reach; absent on logs written before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConfinementRecord {
    pub profile: PermissionProfile,
    pub commands: Commands,
    /// What the session's file tools may write under the checkout.
    pub worktree: Worktree,
    pub network: bool,
}

impl ConfinementRecord {
    /// What the session's confinement allows it, as the log keeps it.
    pub fn of(confinement: &Confinement) -> Self {
        ConfinementRecord {
            profile: confinement.profile,
            commands: confinement.commands,
            worktree: confinement.worktree,
            network: confinement.network,
        }
    }

    /// Whether the checkout is fenced whole: the commands are confined and the
    /// worktree is `Nothing`, so the sandbox admits no command write under it.
    /// The fence refuses a file tool's write under the checkout whatever the
    /// scope admits, since `Fence::judge` admits nothing under a worktree the
    /// confinement does not write. Only a root the confinement holds, such as
    /// the artifact directory or a shared directory, lets such a write through.
    pub fn fences_the_checkout(&self) -> bool {
        self.commands == Commands::Confined && self.worktree == Worktree::Nothing
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
    /// The effort the session was asked to reason with; absent when the
    /// node and its runner left it to the CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<crate::Effort>,
    pub capabilities: Capabilities,
    /// How much of this session the adapter's fence covered — derived
    /// from what it built, never declared. Absent when it built none.
    /// The level travels once, in `capabilities.fence`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fence: Option<Coverage>,
    /// The confinement the session was opened under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confinement: Option<ConfinementRecord>,
    /// The task a loop's session works. Absent for a node's own session,
    /// and on a log written before sessions named their task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The session this one resumes: the same conversation, picked up
    /// after the answer to what it asked. Absent for a session that
    /// opened fresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continues: Option<SessionId>,
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

/// What the engine formatted the moment a session wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WriteFormattedPayload {
    pub session_id: SessionId,
    /// The project command that formatted them.
    pub run: crate::ids::CommandName,
    /// Every file it was handed, relative to the worktree.
    pub paths: Vec<std::path::PathBuf>,
    /// The ones whose content it changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<std::path::PathBuf>,
    pub exit_code: i32,
    /// How long the command took, which the session waited.
    pub duration_ms: u64,
    /// What the command printed, redacted and kept under its hash. Absent
    /// when the store did not keep it, and in logs written before this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ContentHash>,
    /// Whether the bound stopped the command before it finished.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub timed_out: bool,
}

/// A failure of the rail, with the moment it was for and its closed cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RailFailedPayload {
    /// The session the failure belongs to; absent when no session does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// The moment the failed call was for; absent when none could be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<RailEvent>,
    /// The closed reason the rail failed.
    pub cause: RailFailureCause,
}

/// A session that asked to stop with work its close would not take, told
/// why and sent back to it — at most once for one state of that work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StopRefusedPayload {
    pub session_id: SessionId,
    /// What the work stood at: the tree the session left and, for a
    /// node's own session, what the run holds of the documents it owes.
    pub state: ContentHash,
    /// What kept the work from closing, in the order the close judges it.
    pub why: Vec<StopRefusal>,
    /// The task a loop's session works; absent for a node's own session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
}

/// One reason a session's work would not close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopRefusal {
    /// A criterion of its task is red, or a guard after one waits.
    CriteriaRed,
    /// It changed a path outside its scope.
    OutsideScope,
    /// A document its node declares was never handed over.
    ArtifactMissing,
    /// A document was handed over, and the close would refuse it.
    ArtifactRefused,
}

impl StopRefusal {
    /// The word the log and a reader call it by.
    pub fn as_str(self) -> &'static str {
        match self {
            StopRefusal::CriteriaRed => "criteria_red",
            StopRefusal::OutsideScope => "outside_scope",
            StopRefusal::ArtifactMissing => "artifact_missing",
            StopRefusal::ArtifactRefused => "artifact_refused",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageType {
    ToolUse,
    Usage,
    Note,
}

impl AgentMessageType {
    /// The word this kind of message is called by, as the log spells
    /// it.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentMessageType::ToolUse => "tool use",
            AgentMessageType::Usage => "usage",
            AgentMessageType::Note => "note",
        }
    }
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
    display: Option<String>,
    /// What identifies the target, whether or not it can be shown.
    digest: ContentHash,
}

impl ToolTarget {
    /// A path the session acted on: shown as written, which is how a
    /// reader finds the file, and hashed so two edits of one file read
    /// as the same target.
    pub fn of_path(path: &Path) -> Self {
        Self::named(&path.display().to_string())
    }

    /// A name the engine can vouch for: shown as written, and hashed the
    /// way a path is, so two uses of one name read as the same target.
    pub fn named(name: &str) -> Self {
        ToolTarget {
            digest: crate::sha256_hex(name.as_bytes()),
            display: Some(name.to_owned()),
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

    /// The target as a reader may see it, or `None` when the value is the
    /// session's own text.
    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }

    /// What identifies the target, whether or not it can be shown.
    pub fn digest(&self) -> &ContentHash {
        &self.digest
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityDegradedPayload {
    /// The capability the engine consulted and the adapter does not declare.
    pub capability: Capability,
    pub adapter: AdapterId,
    /// What the engine did instead. A log that stored a sentence under
    /// `policy_applied` reads it back as `PolicyApplied::Recorded`.
    #[serde(alias = "policy_applied")]
    #[schemars(with = "Policy")]
    pub policy: PolicyApplied,
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
            policy: PolicyApplied::Known(policy),
        }
    }
}

/// A best-effort step that failed and that the run carried on past: the
/// step, and the cause its source gave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BestEffortFailedPayload {
    pub step: BestEffortStep,
    /// The source's message, verbatim.
    pub cause: String,
}

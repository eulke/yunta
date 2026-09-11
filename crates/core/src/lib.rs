//! Shared domain types for the Yunta workspace.
//!
//! `yunta-core` is the bottom of the dependency graph: every other
//! crate may depend on it, and it depends on nothing else in the workspace.
//! Newtyped identifiers, the `Clock` trait, and error
//! types live here since every other crate needs them.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting, indexing, or panicking.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]
// Tests are the one place a failed assertion is meant to abort. The panic
// family is lifted there by `clippy.toml`; `indexing_slicing` has no such
// switch, so it is lifted in test builds here. Integration tests are
// separate crates neither reaches.
#![cfg_attr(test, allow(clippy::indexing_slicing))]

mod capabilities;
mod clock;
mod config;
pub mod diagnostic;
mod error;
pub mod events;
mod findings;
mod glob;
mod hash;
mod id_source;
mod ids;
mod inputs;
mod ledger;
mod manifest;
mod pack;
pub mod policy;
mod questions;
pub mod schema;
mod secret;
pub mod shape;
mod workflow;
pub mod yaml;

pub use capabilities::{Capabilities, Capability};
pub use clock::{Clock, SystemClock};
pub use config::{
    permission_layer_conflicts, user_state_root, AdapterSettings, BaselineConfig,
    CommandPermissions, ConfigLayer, CoverageConfig, DefaultOnFailure, DefaultsConfig, Env,
    ExecutorKind, ExecutorRegistration, ForgeConfig, GitHubForgeConfig, HomeExpansionError,
    Isolation, LimitsConfig, McpServerConfig, NetworkPermissions, PackExecutorPolicy,
    PackPermissions, PathsConfig, PermissionsConfig, PricingEntry, ProjectConfig,
    PublisherPermissions, RunnerCandidate, SkillsConfig, StorageConfig,
};
pub use diagnostic::{Diagnostic, DocumentKind, DocumentRef, Problem, Report, Subject};
pub use error::{describe, AdapterError, Result};
pub use findings::{FindingEntry, FindingsFile, ProposedCriterionEntry};
pub use glob::{scope_glob, scope_globset};
pub use hash::{sha256_hex, CommitSha, ContentHash};
#[cfg(any(test, feature = "testkit"))]
pub use id_source::SeqIdSource;
pub use id_source::{IdSource, SystemIdSource};
pub use ids::{
    is_path_segment, AdapterId, AgentName, ExecutorName, FindingId, GitHubRepo, InvalidId,
    ModeName, ModelName, NodeId, OptionId, PackName, PackRef, Pid, Publisher, QuestionId,
    Responder, RunId, RunnerName, Seq, SessionId, TaskId,
};
pub use inputs::{InputSpec, InputSpecError};
pub use ledger::{Criterion, Ledger, Task};
pub use manifest::{content_hash, FrozenPaths, Manifest, PackProvenance, RelativeRootError};
pub use pack::{
    stays_inside, PackContents, PackDeclares, PackLock, PackLockEntry, PackManifest,
    PackManifestError, PackRequires, RequiredRunner,
};
pub use policy::ScopeExpansionMode;
pub use questions::{validate_answers, Answer, AnswerType, AnswersFile, Question, QuestionsFile};
pub use secret::Secret;
pub use workflow::{
    ArtifactContextRef, ArtifactKind, ArtifactSpec, Artifacts, CheckBuiltin, CleanupTarget,
    ContextSpec, Coordination, ExternalGate, ForgeKind, HookFailurePolicy, HookStep, Hooks,
    JoinPolicy, KnowledgeLayer, KnowledgeParams, LedgerParams, LoopUntil, McpQueryParams,
    ModeInclude, ModeSpec, MountArtifact, MountSpec, Node, NodeDefaults, NodeIter, NodeKind,
    NodeOutputParams, NodePermissions, OnFailure, OnFinishStep, OnInterrupt, PromptSource,
    RunEventsFilter, RunEventsParams, ScopeExpansion, Workflow, WorkflowIsolation,
};

/// The schema major this binary speaks — what a
/// workflow's `yunta_schema:` range is checked against.
pub const YUNTA_SCHEMA: u32 = 1;

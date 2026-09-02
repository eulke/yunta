//! Shared domain types for the Yunta workspace.
//!
//! `yunta-core` is the bottom of the dependency graph: every other
//! crate may depend on it, and it depends on nothing else in the workspace.
//! Newtyped identifiers, the `Clock` trait, and error
//! types live here since every other crate needs them.

mod capabilities;
mod clock;
mod config;
mod error;
pub mod events;
mod findings;
mod glob;
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
mod workflow;
pub mod yaml;

pub use capabilities::Capabilities;
pub use clock::{Clock, SystemClock};
pub use config::{
    permission_layer_conflicts, user_state_root, AdapterSettings, BaselineConfig,
    CommandPermissions, ConfigLayer, CoverageConfig, DefaultOnFailure, DefaultsConfig,
    ExecutorKind, ExecutorRegistration, ForgeConfig, GitHubForgeConfig, HomeExpansionError,
    Isolation, LimitsConfig, McpServerConfig, NetworkPermissions, PackExecutorPolicy,
    PackPermissions, PathsConfig, PermissionsConfig, PricingEntry, ProjectConfig,
    PublisherPermissions, RunnerCandidate, SkillsConfig, StorageConfig, TelemetryConfig,
    TelemetryProtocol,
};
pub use error::{Result, YuntaError};
pub use findings::{FindingEntry, FindingsFile, ProposedCriterionEntry};
pub use glob::{scope_glob, scope_globset};
#[cfg(any(test, feature = "testkit"))]
pub use id_source::SeqIdSource;
pub use id_source::{IdSource, SystemIdSource};
pub use ids::{
    is_path_segment, AdapterId, AgentName, ExecutorName, FindingId, InvalidId, ModeName, ModelName,
    NodeId, PackName, PackRef, Pid, Publisher, QuestionId, RunId, RunnerName, Seq, SessionId,
    TaskId,
};
pub use inputs::{InputSpec, InputSpecContradiction};
pub use ledger::{Criterion, Ledger, Task};
pub use manifest::{content_hash, sha256_hex, FrozenPaths, Manifest, PackProvenance};
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
    RunEventsParams, ScopeExpansion, Workflow, WorkflowIsolation,
};

/// The schema major this binary speaks — what a
/// workflow's `yunta_schema:` range is checked against.
pub const YUNTA_SCHEMA: u32 = 1;

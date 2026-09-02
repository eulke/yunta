#![forbid(unsafe_code)]

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
mod ids;
mod inputs;
mod ledger;
mod manifest;
mod pack;
mod questions;
mod workflow;

pub use capabilities::Capabilities;
pub use clock::{Clock, SystemClock};
pub use config::{
    permission_layer_conflicts, user_state_root, AdapterSettings, BaselineConfig,
    CommandPermissions, ConfigLayer, CoverageConfig, DefaultOnFailure, DefaultsConfig,
    ExecutorKind, ExecutorRegistration, ForgeConfig, GitHubForgeConfig, Isolation, LimitsConfig,
    McpServerConfig, NetworkPermissions, PackExecutorPolicy, PackPermissions, PathsConfig,
    PermissionsConfig, PricingEntry, ProjectConfig, PublisherPermissions, RunnerCandidate,
    SkillsConfig, StorageConfig, TelemetryConfig, TelemetryProtocol,
};
pub use error::{Result, YuntaError};
pub use ids::{NodeId, RunId, SessionId, TaskId};
pub use inputs::InputSpec;
pub use ledger::{Ledger, Task};
pub use manifest::{content_hash, sha256_hex, FrozenPaths, Manifest, PackProvenance};
pub use pack::{
    PackContents, PackDeclares, PackLock, PackLockEntry, PackManifest, PackRequires, RequiredRole,
};
pub use questions::{validate_answers, Answer, AnswerType, AnswersFile, Question, QuestionsFile};
pub use workflow::{
    ArtifactContextRef, ArtifactKind, ArtifactSpec, Artifacts, CheckBuiltin, CleanupTarget,
    ContextSpec, Coordination, ExternalGate, ForgeKind, HookFailurePolicy, HookStep, Hooks,
    JoinPolicy, KnowledgeLayer, KnowledgeParams, LedgerParams, McpQueryParams, ModeInclude,
    ModeSpec, MountArtifact, MountSpec, Node, NodeDefaults, NodeIter, NodeKind, NodeOutputParams,
    NodePermissions, OnFailure, OnFinishStep, OnInterrupt, PromptSource, RunEventsParams,
    ScopeExpansion, Workflow, WorkflowIsolation,
};

/// The schema major this binary speaks — what a
/// workflow's `yunta_schema:` range is checked against.
pub const YUNTA_SCHEMA: u32 = 1;

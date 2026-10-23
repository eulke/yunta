//! Shared domain types for the Yunta workspace.
//!
//! `yunta-core` is the bottom of the dependency graph: every other crate may
//! depend on it, and it depends on nothing else in the workspace. Its map:
//!
//! - Documents a person or an agent writes, and how they are read: `workflow`,
//!   `config`, `inputs`, `pack`, `tasks`, `spec`, `questions`, `findings`,
//!   `agent`, `template`, `policy`, `schema` and `schema_range`, entering
//!   through `yaml` and judged by `shape` and `diagnostic`.
//! - The log and what a run is frozen with: `events`, `manifest`, `hash`,
//!   `persisted`, and `receipt`, built from the log alone.
//! - What a session may call and write: `port`, `capabilities`, `run_tool`,
//!   `confinement`, `protected`, `glob`, `fence`, `rail` and `expansion`.
//! - What a person is shown and may answer: `shown`, `reserved`, `step` and
//!   `caution`; `best_effort` names a step a run carries on past.
//! - Boundaries a decision is handed rather than reaching for: `clock`,
//!   `id_source`, `process`, `disk`, `durable` and `secret`.
//! - Values every crate shares: `ids`, `nonempty`, `graph`, `text`, `units`,
//!   `shell`, `commit_message` and `error`.

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

pub mod agent;
pub mod best_effort;
mod capabilities;
pub mod caution;
mod clock;
pub mod commit_message;
mod config;
pub mod confinement;
pub mod diagnostic;
pub mod disk;
pub mod durable;
mod error;
pub mod events;
pub mod expansion;
pub mod fence;
mod findings;
mod glob;
pub mod graph;
mod hash;
mod id_source;
mod ids;
pub mod inputs;
mod manifest;
mod nonempty;
mod pack;
pub mod persisted;
pub mod policy;
pub mod port;
pub mod process;
pub mod protected;
mod questions;
pub mod rail;
pub mod receipt;
pub mod reserved;
mod run_tool;
pub mod schema;
mod schema_range;
mod secret;
pub mod shape;
pub mod shell;
pub mod shown;
mod spec;
pub mod step;
mod tasks;
pub mod template;
pub mod text;
pub mod units;
pub mod workflow;
pub mod yaml;

pub use capabilities::{Capabilities, Capability, FenceLevel, ToolNaming};
pub use clock::{Clock, SystemClock};
pub use config::{
    network_asked, permission_layer_conflicts, user_knowledge_dir, user_state_root,
    AdapterSettings, BaselineConfig, CommandPermissions, ConfigKey, ConfigLayer, CoverageConfig,
    DefaultOnFailure, DefaultsConfig, DiskShare, Env, ExecutorKind, ExecutorRegistration,
    ForgeConfig, GitHubForgeConfig, HomeExpansionError, Isolation, LimitsConfig, McpServerConfig,
    NetworkPermissions, PackExecutorPolicy, PackPermissions, PathsConfig, PermissionsConfig,
    PoolConfig, PricingEntry, ProcessSecrets, ProjectConfig, PublisherPermissions, Redactor,
    RunnerCandidate, SecretSource, ShortSecret, SkillsConfig, StorageConfig, MIN_SECRET_CHARS,
    REDACTED,
};
pub use confinement::{temp_dirs, Commands, Confinement, ConfinementInput, EditSandbox, Worktree};
pub use diagnostic::{
    ArtifactFailure, Diagnostic, DocumentRef, Fault, FileProblem, Named, Problem, Report, Rule,
    RuleCode, RuleWords, RunFault, Subject,
};
pub use error::{describe, with_causes, AdapterError, Result, Unbuildable};
pub use events::Unredactable;
pub use findings::fault::FindingFault;
pub use findings::{
    FindingEntry, FindingsFile, FoundEntry, HeldEntry, HeldFindings, InvalidLocation, LineRange,
    Location, ProposedCriterionEntry, RelativePath, Withdrawal,
};
pub use glob::{
    listed_globs, might_overlap, reaches_any, scope_globset, InvalidScopeGlob, ScopeGlob,
};
pub use hash::{git_blob_id, sha256_hex, CommitSha, ContentHash, TreeId};
pub use id_source::{IdSource, SystemIdSource};
pub use ids::{
    is_path_segment, AdapterId, AgentName, CommandName, ExecutorName, FindingId, GitHubRepo,
    InputName, InvalidId, McpServerName, ModeName, ModelName, NodeId, OptionId, PackName, PackRef,
    Pid, Publisher, QuestionId, Responder, RunId, RunnerName, Seq, SessionId, SharedDirVar,
    SkillName, TaskId, WorkflowName,
};
pub use inputs::{InputSpec, InputSpecError, InputsProblem};
pub use manifest::{
    content_hash, Frozen, FrozenPaths, FrozenWorkflow, Manifest, PackProvenance, ProjectIdentity,
    ProvenanceGap, RelativeRootError, RunAdapter,
};
pub use nonempty::{Empty, NonEmpty};
pub use pack::{
    stays_inside, PackContents, PackDeclares, PackLock, PackLockEntry, PackManifest,
    PackManifestError, PackRequires, RequiredRunner,
};
pub use policy::{ScopeCeiling, ScopeExpansionMode};
pub use questions::{Answer, AnswerType, AnswersFile, Question, QuestionFault, QuestionsFile};
pub use run_tool::RunTool;
pub use schema_range::{SchemaRange, SchemaRangeError};
pub use secret::Secret;
pub use spec::{in_repo, Spec, SpecFault, SpecFile, SpecTest, TestFile};
pub use tasks::{
    names_file, passes_by_a_name, Change, Criterion, Decision, Shape, Suite, Task, TaskFault,
    TasksFile,
};
pub use workflow::{
    followers, left_out, node_commands, reroute_sources, ArtifactContextRef, ArtifactKind,
    ArtifactName, ArtifactRefId, ArtifactSpec, Artifacts, Ask, Because, CheckBuiltin,
    CleanupTarget, CliConfig, ContextFile, ContextSpec, Coordination, DistillArtifact, Effort,
    ExternalGate, ForgeKind, HookFailurePolicy, HookStep, Hooks, JoinPolicy, KnowledgeLayer,
    KnowledgeParams, LeftOut, LoopUntil, McpQueryParams, ModeInclude, ModeSpec, MountArtifact,
    MountSpec, Node, NodeDefaults, NodeIter, NodeKind, NodeOutputParams, NodePermissions,
    NodeScope, OnFailure, OnFinishStep, OnInterrupt, PromptSource, ReservedIdentity, Resolved,
    RunCommand, RunEventsFilter, RunEventsParams, ScopeExpansion, TasksParams, UnknownArtifactKind,
    Workflow, WorkflowFault, ARTIFACTS_DIR, OBJECTS_DIR,
};

/// The schema major this binary speaks — what a
/// workflow's `yunta_schema:` range is checked against.
pub const YUNTA_SCHEMA: u32 = 1;

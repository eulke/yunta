//! The workflow engine: DAG scheduler, verification cycle, resumability.
//! `yunta-engine` never depends on rusqlite/sqlx
//! directly and never contains CLI-specific knowledge — those
//! are enforced by the crate graph itself, not by convention.
//!
//! This crate anchors the workspace dependency graph
//! (core ← storage/adapters ← engine ← cli), keeping it compiling and testable.

// The run contract needs a process layer that owns whole process trees:
// every subprocess in its own process group, interrupted and killed
// with its descendants, and lock liveness by signal. That layer is
// POSIX; a build for another platform would compile and then leave
// orphans on cancellation, the one degradation the contract forbids.
#[cfg(not(unix))]
compile_error!(
    "yunta-engine builds for unix targets only: its process management (process groups, \
     signals, lock liveness) is POSIX, and a binary that cannot cancel a whole process tree \
     cannot keep the run contract. Windows returns as a target once a process layer with the \
     same guarantees exists and its cancellation tests pass there."
);

mod artifacts;
mod catalog;
mod check;
mod events_export;
mod findings;
mod human_interaction;
mod inputs;
mod ledger;
pub mod lock;
mod manifest;
mod modes;
mod pack_audit;
mod pack_requires;
mod permissions;
pub mod process;
mod process_registry;
mod progress;
mod questions;
mod receipt;
mod replay;
mod run;
mod run_tools;
mod runner;
mod scope;
pub mod scope_expansion;
mod skills;
mod stats;
mod task_cycle;
mod template;
mod verification_effectiveness;
mod worktree;

pub use artifacts::{close_artifacts, ArtifactError, VerifiedArtifact};
pub use catalog::{
    installed_publishers, origin_of, packs_for_publisher, resolve_workflow, CatalogError,
    PublisherPacks, ResolvedWorkflow, WorkflowOrigin,
};
pub use check::{
    check, check_warnings, check_workflow_refs, CheckError, CheckWarning, SchemaRangeError,
};
pub use events_export::{render_events_jsonl, EventsExportError};
pub use findings::{inherited_findings, register as register_findings, FindingsError};
pub use human_interaction::{HumanInteraction, NoInteraction, QuestionsReply};
pub use inputs::{resolve_inputs, InputsError};
pub use ledger::{register, LedgerError};
pub use manifest::{build_manifest, ManifestError};
pub use modes::{dependencies_in_mode, mode_included_nodes};
pub use pack_audit::{
    audit_pack, NodeAudit, PackAudit, PromptReadError, PromptText, WorkflowAudit,
};
pub use pack_requires::{check_pack_requires, PackRequiresGap};
pub use permissions::command_violation;
pub use process_registry::{read_registry, registry_path, EngineProcessFile, ProcessRegistry};
pub use progress::render_progress;
pub use questions::{register as register_questions, QuestionsError};
pub use receipt::{
    build_receipt, fan_out_groups, render_json as render_receipt_json,
    render_markdown as render_receipt_markdown, BaselineSummary, CostSummary, CriteriaSummary,
    CriterionEntry, EventChainStatus, Receipt, ReceiptError, RunnerUsage, ScopeSummary,
};
pub use replay::{
    dedup_findings, derive, unknown_kind_counts, NodeState, RunState, UnknownKindCount,
};
pub use run::{
    create_promotion_successor, create_run, current_escalation, execute_run, read_manifest,
    resolve_gate, session_token_budget, BirthArtifact, CreateRunParams, ManifestReadError,
    Predecessor, PromotionSuccessor, ResolveGateError, RunEnv, RunError, RunReport, RunRoots,
    RunTerminal,
};
pub use run_tools::{consolidate_blackboard, open_session_listener, RunToolsHost, RunToolsSession};
pub use runner::{resolve_runner, ResolvedRunner, RunnerError};
pub use scope::{scope_check, ScopeCheckError, ScopeCheckResult};
pub use stats::{
    budget_p90_warning, compute_run_stats, prior_estimation, run_summary, NodeStat, Percentiles,
    PriorEstimation, RunStats, RunSummary, MIN_SAMPLES_FOR_ESTIMATION,
};
pub use task_cycle::{
    post_check, pre_check, run_task, AttemptEnv, AttemptRecord, CriterionRun, DispatchOutcome,
    Memo, PreCheckOutcome, ScopeGovernance, SessionObserver, SessionSetup, TaskCycleError,
    TaskCycleReport, TaskOutcome, DEFAULT_MAX_RETRIES,
};
pub use template::{render_template, template_variables, TemplateError};
pub use verification_effectiveness::{
    analyze as analyze_verification_effectiveness, AlwaysApprovedGate, AlwaysFirstTryTasks,
    NeverRedCriterion, NeverTriggeredReroute, VerificationFindings,
    MIN_SAMPLES as VERIFICATION_MIN_SAMPLES,
};
pub use worktree::{
    cleanup_worktree, prepare_worktree, release_worktree, WorktreeCleanup, WorktreeError,
    WorktreePrepared,
};

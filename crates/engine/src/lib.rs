//! The workflow engine: DAG scheduler, verification cycle, resumability.
//! `yunta-engine` never depends on rusqlite/sqlx directly and never contains
//! CLI-specific knowledge — the crate graph enforces both, not convention. It
//! reaches an adapter only through the port `yunta-core` declares, and the CLI
//! is the only shipped crate above it; the test harness `yunta-testkit` builds
//! on it too. What a run is, as its log says, is `yunta-replay`'s: the engine
//! drives a run and writes its log, and reads back what replay derives from
//! it. Its map:
//!
//! - Birth and resume: `run` creates a run and drives its loop, from what
//!   `manifest` freezes and `frozen` keeps; `inputs`, `catalog`,
//!   `agents`, `skills` and `runner` resolve what a workflow names, and
//!   `run_dir` and `session_dir` lay out the working space a run keeps on
//!   disk, beside what a reader resolves there (`yunta_core::run_dir`).
//! - A node's work: `task_cycle` runs a task through its checks, over what
//!   `tasks` means to the run; `scope`, `scope_expansion` and `permissions`
//!   judge what it wrote; `answers` and `reserved` carry a person's decisions,
//!   asked through core's person port; `findings` and `artifacts` keep what nodes report
//!   and hand over; `check` is `yunta check`.
//! - The log and what reads it: `run_log` is the one way an event reaches it,
//!   and a read of it, replayed, is its `RunView`; each event reaches a
//!   surface through core's observer port as it is appended, and
//!   `events_export` reads what the log says.
//! - Sessions' tools: `run_tools` serves a node session's MCP server, in the
//!   shape `yunta-mcp-protocol` says every MCP result of Yunta takes.
//! - What sessions say: `said` holds it for whoever watches the run, in this
//!   process and through the run's socket, and never on the log.
//! - Checkouts: `worktree` isolates and pools them; `wakefulness` notices a
//!   host that slept.
//! - Boundaries: `git`, `process`, `process_registry`, `lock`, `run_hold` and `pace`.
//! - Packs: `pack_audit` and `pack_requires`.

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

// Process management needs a layer that owns whole process trees:
// every subprocess in its own process group, interrupted and killed
// with its descendants, and lock liveness by signal. That layer is
// POSIX; a build for another platform would compile and then leave
// orphans on cancellation, the one degradation that is never acceptable.
#[cfg(not(unix))]
compile_error!(
    "yunta-engine builds for unix targets only: its process management (process groups, \
     signals, lock liveness) is POSIX; a target without it could not cancel a whole \
     process tree. Windows returns as a target once a process layer with the same \
     guarantees exists and its cancellation tests pass there."
);

mod agents;
mod answers;
mod artifacts;
mod catalog;
mod check;
mod events_export;
mod findings;
mod frozen;
pub mod git;
mod inputs;
pub mod lock;
mod manifest;
pub mod pace;
mod pack_audit;
mod pack_requires;
mod permissions;
pub mod process;
mod process_registry;
mod reserved;
mod run;
pub mod run_dir;
mod run_hold;
mod run_log;
mod run_tools;
mod runner;
pub mod said;
mod scope;
pub mod scope_expansion;
mod session_dir;
mod skills;
mod task_cycle;
mod tasks;
mod wakefulness;
mod worktree;

pub use answers::{
    answer_questions, AnswerQuestionsError, AnswersError, Recorded as AnswersRecorded,
    Reply as AnswersReply,
};
pub use artifacts::shown::{documents as shown_documents, produced as produced_documents};
pub use artifacts::store::{ObjectStore, StoreError};
pub use artifacts::{
    close_artifacts, AcceptError, ArtifactContent, ArtifactFault, ArtifactIntegrity, StagedHash,
    VerifiedArtifact,
};
pub use catalog::{
    installed_publishers, origin_of, packs_for_publisher, resolve_workflow, CatalogError,
    PublisherPacks, ResolvedWorkflow, WorkflowOrigin,
};
pub use check::{
    check, check_context_files, check_mode_start, check_mounted, check_warnings,
    check_workflow_refs, programs_named, AdapterFacts, ContextFilesCheck, RefsCheck, RunTreeOrigin,
};
pub use events_export::{render_events_jsonl, EventsExportError};
pub use findings::inherited_findings;
pub use git::GitError;
pub use inputs::{resolve_inputs, InputsError, ResolvedInputs};
pub use manifest::{build_manifest, FrozenRun, ManifestError};
pub use pack_audit::audit_pack;
pub use pack_requires::{check_pack_requires, retired_permissions_sentence, PackRequiresGap};
pub use permissions::command_violation;
pub use process_registry::{
    read_engine, read_registry, registry_path, running_groups, EngineProcessFile, ProcessRegistry,
    Registry,
};
pub use run::schedule::{decide, follows_build, Decision, Policy as SchedulingPolicy};
pub use run::{
    awaits_decision, close_run, create_promotion_successor, create_run, current_escalation,
    execute_run, read_manifest, record_pause_after_crash, resolve_gate, run_redactor,
    session_token_budget, BirthArtifact, BirthOrigin, CallerInfra, CloseRunError, CreateRunParams,
    ManifestReadError, OwedMeasurement, Predecessor, PromotionSuccessor, ResolveGateError, RunEnv,
    RunError, RunReport, RunRoots, RunTerminal,
};
pub use run_hold::{contention_for, hold_refused, hold_run, RunHold};
pub use run_tools::{
    consolidate_blackboard, open_session_listener, HostOf, NodeScopeAccess, RunTool,
    RunToolsAccess, RunToolsHost, RunToolsSession, TaskAccess, TaskExpansion,
};
pub use runner::{resolve_runner, ResolvedRunner, RunnerError};
pub use scope::{
    audit, audits, changed_between, changed_since, fence_breach, node_scope_in_force, violations,
    Breach, Ceiling, ScopeCheckError, ScopeCheckResult,
};
pub use task_cycle::{
    post_check, pre_check, run_task, surprises, AttemptEnv, AttemptRecord, CriterionRun,
    DispatchOutcome, KeepsOutput, Memo, PostCheck, ScopeGovernance, SessionObserver, SessionSetup,
    Surprise, TaskCause, TaskCycleError, TaskCycleReport, TaskOutcome,
};
pub use tasks::{held_of, judged_task, measured};
pub use wakefulness::SETTLE_AFTER_SUSPENSION;
pub use worktree::{
    adopt_unit_branches, adoptable_unit_branches, capture_tree, carry_work, cleanup_worktree,
    commit_tree, commit_work, forget_run_units, hand_over_run_checkout, hand_over_worktree,
    head_commit, head_tree, land, pool_home, prepare_worktree, prune_worktrees, rebase_onto,
    release_run_checkout, release_unit_checkouts, release_worktree, remove_checkout,
    remove_old_checkouts, snapshot_commit, still_the_runs, Birth, Carried, CheckoutPool, Lease,
    OldCheckout, PoolReport, Rebase, RunWorktree, Tidied, Unit, UnitHome, UnitId, UnitRef,
    WorktreeCleanup, WorktreeError, WorktreeIntegrity, WorktreePrepared,
};

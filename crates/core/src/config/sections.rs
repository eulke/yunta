//! The sections of a config layer that are plain data: `runners`,
//! `adapters`, `mcp_servers`, `forge`, `storage`, `project`, `paths`,
//! `defaults`, `limits`, `pricing`, `baseline`, `coverage` and `skills`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::{AdapterId, AgentName, ExecutorName, GitHubRepo, ModelName, RunnerName};
use crate::workflow::OnInterrupt;

/// One binding candidate for a role in `runners:`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunnerCandidate {
    pub adapter: AdapterId,
    pub model: ModelName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentName>,
}

/// One server in `mcp_servers:` — the reference config's own
/// shape: a streamable-HTTP endpoint plus the *name* of an env var
/// carrying the bearer token, never the token itself (secrets are
/// env var names in config, values only ever come from the process
/// environment at resolve time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpServerConfig {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env: Option<String>,
}

/// `forge:` — the team's forge, so `check` can tell a
/// `kind: gate` with `external:` apart from one with nowhere to
/// actually publish. GitHub only in v1, same "closed enum over an open
/// abstraction" stance `ForgeKind` takes on the workflow side — a
/// second forge is a new field here, not a schema break. `token_env`
/// names an env var, never carries the token itself (same
/// convention as `McpServerConfig::auth_env`); its absence at *runtime*
/// (not at `check` time — see [`GitHubForgeConfig`]) is exactly what
/// makes a machine with no credentials at all work:
/// degrade to console, don't refuse to exist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubForgeConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitHubForgeConfig {
    pub repo: GitHubRepo,
    /// The environment variable holding the token; its value never
    /// enters the config.
    pub token_env: String,
}

/// Adapter-specific settings: a portable binary override plus the
/// reference config's opaque `adapter_settings` map — this is
/// only for what has NO portable expression; model/agent/permissions
/// are typed request fields precisely so this stays small. Passed
/// through to `SessionRequest.adapter_settings` untouched; the adapter
/// validates what it can in `probe()` and rejects what it doesn't know.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_settings: Option<serde_json::Map<String, serde_json::Value>>,
}

/// `storage:` — SQLite is the only backend.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

/// `project:` — backs the `{{project.*}}` template namespace, the
/// reference config's own three fields. Currently read-only data for
/// templates, nothing here drives behavior yet (`base_branch` isn't
/// consulted by any re-route/PR logic).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_prefix: Option<String>,
}

/// `paths:` — where run/worktree state lives. `YUNTA_HOME` is
/// an environment override applied when resolving the merged config, not
/// a field of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<PathBuf>,
}

/// How a first-level run isolates its working tree from the checkout
/// that started it. `worktree` (default) gives each run its
/// own `git worktree`; `none` operates directly on the given checkout,
/// legitimate for watching an agent edit live or for CI already inside
/// an ephemeral container. `inherit` (sub-runs only) isn't a value
/// here — a first-level run has no parent to inherit from — and
/// `container` isn't a schema value at all (not yet designed).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    #[default]
    Worktree,
    None,
}

/// `defaults:` — the reference config's whole group. Each field
/// has its consumer: `isolation`, `max_parallel_nodes`,
/// `on_interrupt`, `runner` (a node that declares none),
/// `timeout_minutes` (`Budget.timeout`), `on_failure` (only `pause` is
/// built — `check` refuses the others rather than accepting them
/// silently).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefaultsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<Isolation>,
    /// The runner a node without `runner:` resolves through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<RunnerName>,
    /// Per-session wall-clock budget (`Budget.timeout`), in minutes —
    /// the granularity the reference schema uses for whole sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u64>,
    /// What a failed node without its own `on_failure:` does. Only
    /// `pause` (today's behavior) is built; `check` refuses the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<DefaultOnFailure>,
    /// How many DAG nodes with no dependency on each other the scheduler
    /// may run at once. Absent means the schema's own default of
    /// `1`, not "unbounded" — the same rationale applies to
    /// `concurrency` in loops: nobody
    /// should discover parallel token spend by reading the bill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel_nodes: Option<u32>,
    /// Fallback `on_interrupt` a node without its own
    /// override resolves to. Same default (`restart_node`) as the
    /// schema's own, so absent here changes nothing either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_interrupt: Option<OnInterrupt>,
}

/// `defaults.on_failure` values (reference schema). Only `Pause` has an
/// implementation — the enum still parses all three so the reference
/// config round-trips, and `check` names the unimplemented ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefaultOnFailure {
    Pause,
    Abort,
    Continue,
}

/// One `pricing:` entry (reference shape): a struct rather than a
/// bare number so a later per-direction price is a field addition, not
/// a schema break.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PricingEntry {
    pub cost_per_1k_tokens: f64,
}

/// `limits:` — declared budgets and guards. Every field is
/// optional: an absent limit means "no cap", never a hidden default —
/// except where the reference schema itself names one
/// ([`ConfigLayer::resolved_max_loop_iterations`](super::ConfigLayer::resolved_max_loop_iterations),
/// [`ConfigLayer::resolved_inline_context_bytes`](super::ConfigLayer::resolved_inline_context_bytes)), and that default
/// lives here and nowhere else. Budgets are advisory ceilings the engine
/// enforces by escalation/diagnostic, never OS enforcement.
///
/// Canonical integer form is `2000000` — the YAML parser (YAML 1.2) resolves
/// `2_000_000` as a *string*, which fails the parse loudly instead of
/// silently becoming an unlimited run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    /// Run-wide token budget: exceeded → escalation
    /// (`continue`/`abort`), or `run_paused { reason: budget }` with no
    /// surface to ask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_per_run: Option<u64>,
    /// Ceiling on loop-node iterations — the only net under a tasks document
    /// whose state oscillates forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_loop_iterations: Option<u32>,
    /// Best-effort cap on simultaneously non-terminal runs, checked at
    /// run creation — a soft budget, not a safety limit (two concurrent
    /// `yunta run` invocations can both pass the check).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_runs: Option<u32>,
    /// Ceiling on workflow-invoking-workflow nesting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_workflow_depth: Option<u32>,
    /// How many files a `rules`-mode scope expansion may touch before it
    /// is denied as no longer "a small, adjacent set" — absent means the
    /// reference default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_expansion_files: Option<usize>,
    /// Guard against runaway artifacts at close: an artifact over
    /// this size fails the node with a diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_artifact_bytes: Option<u64>,
    /// Context sources at or under this size are inlined into the
    /// prompt; larger ones are referenced by path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_context_bytes: Option<u64>,
}

/// `baseline:` — backs the `baseline_compare` check kind; the suite the engine
/// runs and re-runs to catch regressions ("cero regresiones" as a data
/// comparison, never an agent's claim).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaselineConfig {
    pub suite: String,
}

/// `coverage:` — backs the `coverage_gate` check kind; `cmd`'s stdout must
/// contain a bare percentage (`NN[.NN]%`); the last match found is taken
/// as the measured coverage. Not specified any more precisely than
/// "measured and compared by the engine" — a permissive,
/// documented convention rather than inventing a stricter parsing
/// contract with no source to check it against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoverageConfig {
    pub cmd: String,
    pub threshold: f64,
}

/// `skills:` — currently carries only `executors`, the one
/// sub-field `kind: executor` needs to resolve its own `executor:` name
/// to a binary on disk. `paths`/`always` (skill discovery and injection
/// into a node's assembled context) are context-assembly work, with
/// no consumer yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkillsConfig {
    #[serde(default)]
    pub executors: Vec<ExecutorRegistration>,
    /// Directories skill names resolve against, in order — repo first,
    /// per the reference config. Absent means the convention
    /// default, `.yunta/skills` (where `yunta init` installs the
    /// mechanism skill).
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    /// Skill names mounted on every session, before any node's own
    /// list.
    #[serde(default)]
    pub always: Vec<String>,
}

/// One `skills.executors:` entry — `name` is what a `kind: executor`
/// node's own `executor:` field references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRegistration {
    pub name: ExecutorName,
    pub kind: ExecutorKind,
    pub path: PathBuf,
}

/// Closed at `binary` today — `wasm` is reserved as a future
/// additive variant once demand for it shows up, so this is an enum even
/// with a single variant, not a bare string that would silently accept
/// anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    Binary,
}

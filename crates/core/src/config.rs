//! Layered config types: every group of the
//! reference config parses and round-trips — `runners`, `adapters`,
//! `mcp_servers`, `skills`, `baseline`/`coverage`, `storage`, `limits`,
//! `paths`, `defaults`, `permissions`, `pricing`, `forge`, `secrets`,
//! `telemetry` (the one sanctioned parse-and-hold group — inert until
//! the telemetry exporter is built) and `version`. Each field
//! entered with its consumer or an explicit refusal in `check` — never
//! accepted and silently ignored.
//!
//! Merge semantics: maps merge key by key, more specific layer
//! wins per key; arrays (like a role's candidate list) replace wholesale
//! rather than concatenate. Precedence is repo > usuario > org — with one
//! deliberate exception: `permissions` inverts it, the org
//! layer is a ceiling and lower layers only narrow (see
//! [`PermissionsConfig`] and [`permission_layer_conflicts`]).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::{
    AdapterId, AgentName, ExecutorName, GitHubRepo, ModelName, Publisher, RunnerName,
};
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

/// Rewrites `path` in place when it starts with `~`: `~` alone becomes
/// `home`, `~/rest` becomes `home/rest`; `~user/...` is refused.
fn expand_path(
    path: &mut PathBuf,
    home: Option<&std::path::Path>,
    field: &str,
) -> Result<(), HomeExpansionError> {
    let Some(text) = path.to_str() else {
        return Ok(());
    };
    if !text.starts_with('~') {
        return Ok(());
    }
    let rest = &text[1..];
    if !(rest.is_empty() || rest.starts_with('/')) {
        return Err(HomeExpansionError::OtherUser {
            field: field.to_string(),
            path: text.to_string(),
        });
    }
    let Some(home) = home else {
        return Err(HomeExpansionError::NoHome {
            field: field.to_string(),
            path: text.to_string(),
        });
    };
    *path = home.join(rest.trim_start_matches('/'));
    Ok(())
}

/// The ambient environment a run resolves against, captured once at a
/// shell boundary so no code below the boundary reads the process itself —
/// core stays pure, and a test injects a value instead of mutating the
/// process. A shell (the CLI) fills this from `std::env`; everything else
/// only reads the fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `$HOME` — the user's home directory.
    pub home: Option<PathBuf>,
    /// `$YUNTA_HOME` — the state-root override.
    pub yunta_home: Option<PathBuf>,
    /// `$YUNTA_ORG_CONFIG` — the org config path override.
    pub org_config: Option<PathBuf>,
    /// Variables layered onto every subprocess the run spawns (bash nodes,
    /// hooks, executors) on top of the inherited environment — a test
    /// prepends a stub directory to `PATH` here instead of mutating the
    /// process. Empty in production: subprocesses inherit the run's own
    /// environment unchanged.
    pub subprocess_vars: Vec<(String, String)>,
}

/// The user state root: `$YUNTA_HOME` when set, otherwise `~/.yunta`.
/// `None` only when neither is known. Reads the injected [`Env`], never the
/// process, so the CLI (which layers `config.yaml` from it) and the engine
/// (which reads `knowledge/` from it at context-resolution time) resolve
/// the same "user layer" from one value neither of them read live.
pub fn user_state_root(env: &Env) -> Option<PathBuf> {
    if let Some(root) = &env.yunta_home {
        return Some(root.clone());
    }
    env.home.as_ref().map(|home| home.join(".yunta"))
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

/// `telemetry:` — parsed so the reference config round-trips;
/// **inert until the OTel exporter is built**, and the reference text itself says so:
/// this is the one sanctioned parse-and-hold group.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<TelemetryProtocol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryProtocol {
    Grpc,
    Http,
}

/// `limits:` — declared budgets and guards. Every field is
/// optional: an absent limit means "no cap", never a hidden default —
/// except where the reference schema itself names one
/// ([`ConfigLayer::resolved_max_loop_iterations`],
/// [`ConfigLayer::resolved_inline_context_bytes`]), and that default
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
    /// Ceiling on loop-node iterations — the only net under a ledger
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

/// `permissions:` — ONE model of ceilings, not
/// loose mechanisms: each level may only narrow the one above, never
/// loosen it. Unlike every other config group (repo > user > org), the
/// org layer rules here and lower layers only restrict further — without
/// that inversion, governance is theater: any repo could undo it.
///
/// This is governance, not a sandbox: an agent
/// with write access can route around a textual pattern by writing a
/// script and running it. The model stops the accident and the careless
/// pack, and leaves an auditable trail of the deliberate attempt — real
/// isolation belongs to the execution environment, never to Yunta.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PermissionsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<CommandPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<PackPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_expansion: Option<ScopeExpansionPermissions>,
}

/// `permissions.scope_expansion`: the layered ceiling
/// over how a loop node may let its tasks grow past declared scope.
/// `max_mode` is the most *permissive* node-level `scope_expansion.mode`
/// the layer allows (`rules < ask < deny` in severity) — the same
/// only-narrowing model every other `permissions` group follows:
/// merge keeps the strictest declared ceiling, a lower layer softening
/// it is a reported conflict, and a node declaring a mode over the
/// merged ceiling fails `check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeExpansionPermissions {
    pub max_mode: crate::policy::ScopeExpansionMode,
}

/// `permissions.commands` — patterns matched against every hook, criterion,
/// bash node and executor command right before it runs. Empty
/// `allow` = denylist mode (everything not denied runs); a non-empty
/// `allow` switches to a strict, opt-in allowlist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

/// `permissions.packs` — governance over pack contents. Parsed
/// and merged here; *enforced* at `pack add`/check once pack support
/// lands fully — a key without its consumer yet, kept
/// because the org ceiling file is one document and its schema shouldn't
/// dribble in piecemeal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackPermissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executors: Option<PackExecutorPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishers: Option<PublisherPermissions>,
}

/// `allow | prompt | deny`, strictly ordered: `Deny` is the narrowest,
/// `Allow` the loosest — the ceiling merge keeps the strictest across
/// layers. `prompt` asks for confirmation at `yunta pack add`,
/// never mid-run: runs are headless, humans interact through gates only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackExecutorPolicy {
    Allow,
    Prompt,
    Deny,
}

impl PackExecutorPolicy {
    fn strictness(self) -> u8 {
        match self {
            PackExecutorPolicy::Allow => 0,
            PackExecutorPolicy::Prompt => 1,
            PackExecutorPolicy::Deny => 2,
        }
    }
}

/// `permissions.packs.publishers` — `allow` empty means every publisher
/// is accepted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublisherPermissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<Publisher>,
}

/// `permissions.network` — declarative ONLY: `default: false`
/// activates no sandboxing whatsoever. It exists for policy and audit; an
/// executor that wants to actually enforce it does so on its own. Policy
/// ≠ capability ≠ OS enforcement — Yunta core never promises the third.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkPermissions {
    pub default: bool,
}

/// One config layer as parsed from a single file (project/user/org), and
/// also the type of the merged result — merging never needs to invent
/// fields, only combine what layers actually set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigLayer {
    /// `version: 1` (reference config) — the layer file's own format
    /// version. The loader refuses any value this binary doesn't speak.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runners: Option<BTreeMap<RunnerName, Vec<RunnerCandidate>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapters: Option<BTreeMap<AdapterId, AdapterSettings>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<BTreeMap<String, McpServerConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<PathsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defaults: Option<DefaultsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<BaselineConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsConfig>,
    /// `pricing:` — `{model: cost_per_1k_tokens}`, an
    /// optional currency conversion `yunta stats` and the receipt add
    /// *alongside* their token figures, never in place of them. Absent
    /// means everything stays in tokens — the engine has no opinion of
    /// its own on what a token costs, and never invents one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<BTreeMap<String, PricingEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<ForgeConfig>,
    /// `secrets:` — env var *names* a session may receive; values
    /// only ever come from the process environment at spawn time, and
    /// nothing undeclared reaches a session's env at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetryConfig>,
}

/// A path in a config layer that starts with `~` and cannot be
/// expanded: there is no home directory to expand it against, or the
/// form is one this schema does not read (`~user/...`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HomeExpansionError {
    #[error("`{field}` is `{path}` but no home directory is known — set HOME (or YUNTA_HOME for the state root) or write the path in full")]
    NoHome { field: String, path: String },
    #[error(
        "`{field}` is `{path}` — only `~` and `~/...` expand; write another user's home in full"
    )]
    OtherUser { field: String, path: String },
}

impl ConfigLayer {
    /// Expands a leading `~` in every path this layer declares —
    /// `adapters.<id>.binary`, `storage.path`, `paths.runs`,
    /// `paths.worktrees`, `skills.paths[]` — against `home`, so no
    /// consumer ever sees a literal `~`. `home: None` makes any such path
    /// an error naming the field.
    pub fn expand_home(
        &mut self,
        home: Option<&std::path::Path>,
    ) -> Result<(), HomeExpansionError> {
        if let Some(adapters) = &mut self.adapters {
            for (id, settings) in adapters.iter_mut() {
                if let Some(binary) = &mut settings.binary {
                    expand_path(binary, home, &format!("adapters.{id}.binary"))?;
                }
            }
        }
        if let Some(path) = self
            .storage
            .as_mut()
            .and_then(|storage| storage.path.as_mut())
        {
            expand_path(path, home, "storage.path")?;
        }
        if let Some(paths) = &mut self.paths {
            if let Some(runs) = &mut paths.runs {
                expand_path(runs, home, "paths.runs")?;
            }
            if let Some(worktrees) = &mut paths.worktrees {
                expand_path(worktrees, home, "paths.worktrees")?;
            }
        }
        if let Some(skills) = &mut self.skills {
            for (index, path) in skills.paths.iter_mut().enumerate() {
                expand_path(path, home, &format!("skills.paths[{index}]"))?;
            }
        }
        Ok(())
    }

    /// Merges layers in increasing order of precedence — pass
    /// `[org, user, repo]` so the last one's keys win (the default
    /// precedence; `permissions` inverts this).
    pub fn merge_layers(layers: impl IntoIterator<Item = ConfigLayer>) -> ConfigLayer {
        layers.into_iter().fold(ConfigLayer::default(), merge)
    }

    /// `defaults.isolation`, with the schema's own default (`worktree`)
    /// applied — the one place that default lives, so nothing downstream
    /// re-invents "absent means what?".
    pub fn resolved_isolation(&self) -> Isolation {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.isolation)
            .unwrap_or_default()
    }

    /// `defaults.max_parallel_nodes`, with the schema's own default (`1`,
    /// sequential — same place the default lives, same reasoning as
    /// `resolved_isolation`).
    pub fn resolved_max_parallel_nodes(&self) -> u32 {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.max_parallel_nodes)
            .unwrap_or(1)
    }

    /// `defaults.on_interrupt`, with the schema's own default
    /// (`restart_node`) applied — a node's own `on_interrupt` still wins
    /// over this when it declares one.
    pub fn resolved_on_interrupt(&self) -> OnInterrupt {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.on_interrupt)
            .unwrap_or_default()
    }

    /// `defaults.timeout_minutes` as a session `Budget.timeout` — no
    /// hidden default: absent means unlimited, exactly as before.
    pub fn resolved_session_timeout(&self) -> Option<std::time::Duration> {
        self.defaults
            .as_ref()
            .and_then(|defaults| defaults.timeout_minutes)
            .map(|minutes| std::time::Duration::from_secs(minutes * 60))
    }

    /// `limits.max_loop_iterations`, with the reference default (`12`)
    /// applied — the only net under a ledger whose state oscillates
    /// forever, so "absent" means the reference cap, never "unbounded".
    /// `limits.max_workflow_depth`, with the reference default (`4`)
    /// applied — how many `kind: workflow` nesting levels below the
    /// root run are allowed (a configurable maximum depth, checked
    /// statically at graph-build time and again at runtime when a
    /// child run is created).
    pub fn resolved_max_workflow_depth(&self) -> u32 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_workflow_depth)
            .unwrap_or(4)
    }

    pub fn resolved_max_loop_iterations(&self) -> u32 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_loop_iterations)
            .unwrap_or(12)
    }

    /// `limits.max_expansion_files`, with the reference default (`5`)
    /// applied — the ceiling above which a `rules`-mode scope expansion
    /// is no longer "a small, adjacent set" and is denied. Same "the
    /// default lives here" convention as [`ConfigLayer::resolved_isolation`].
    pub fn resolved_max_expansion_files(&self) -> usize {
        self.limits
            .as_ref()
            .and_then(|limits| limits.max_expansion_files)
            .unwrap_or(5)
    }

    /// `limits.inline_context_bytes`, with the reference default
    /// (`32000`) applied — same "the default lives here" convention as
    /// [`ConfigLayer::resolved_isolation`].
    pub fn resolved_inline_context_bytes(&self) -> u64 {
        self.limits
            .as_ref()
            .and_then(|limits| limits.inline_context_bytes)
            .unwrap_or(32_000)
    }
}

fn merge(base: ConfigLayer, more_specific: ConfigLayer) -> ConfigLayer {
    ConfigLayer {
        version: more_specific.version.or(base.version),
        // Union, order-preserving: every layer's declared secret names
        // stand — a repo can add names, never silently drop the org's.
        secrets: {
            let mut secrets = base.secrets;
            for name in more_specific.secrets {
                if !secrets.contains(&name) {
                    secrets.push(name);
                }
            }
            secrets
        },
        telemetry: more_specific.telemetry.or(base.telemetry),
        runners: merge_map_replacing_values(base.runners, more_specific.runners),
        adapters: merge_map_of_fields(
            base.adapters,
            more_specific.adapters,
            merge_adapter_settings,
        ),
        mcp_servers: merge_maps(
            base.mcp_servers,
            more_specific.mcp_servers,
            |_base, more| more,
        ),
        // Per-model, same as `mcp_servers`: a repo layer overriding one
        // model's price doesn't discard the rest a user/org layer priced.
        pricing: merge_maps(base.pricing, more_specific.pricing, |_base, more| more),
        // Whole-group replace, same as `baseline`/`coverage`: one forge
        // per repo in practice, nothing internal to merge field-by-field.
        forge: more_specific.forge.or(base.forge),
        storage: merge_fields(base.storage, more_specific.storage, merge_storage_config),
        project: merge_fields(base.project, more_specific.project, merge_project_config),
        paths: merge_fields(base.paths, more_specific.paths, merge_paths_config),
        defaults: merge_fields(base.defaults, more_specific.defaults, merge_defaults_config),
        // Budgets, not permissions: normal precedence (repo > user >
        // org), field by field — the inverted ceiling merge below is
        // exclusive to `permissions`.
        limits: merge_fields(base.limits, more_specific.limits, merge_limits_config),
        // Every field in these two is required (no internal optionality
        // to merge field-by-field) — a more specific layer replaces the
        // whole group wholesale, same as `runners`' candidate arrays.
        baseline: more_specific.baseline.or(base.baseline),
        coverage: more_specific.coverage.or(base.coverage),
        skills: more_specific.skills.or(base.skills),
        // The deliberate inversion: `merge_layers` folds org
        // first, so `base` here is always the HIGHER layer for
        // permissions — the ceiling. The lower layer only ever narrows
        // the result; a loosening attempt is surfaced as an error by
        // [`permission_layer_conflicts`], never silently merged away.
        permissions: merge_permissions(base.permissions, more_specific.permissions),
    }
}

/// Ceiling merge for `permissions`: the effective model is the
/// most restrictive combination of both layers, computed conservatively —
/// even when a lower layer *tried* to loosen (a check error via
/// [`permission_layer_conflicts`]), the runtime model never runs anything
/// the ceiling denied.
fn merge_permissions(
    ceiling: Option<PermissionsConfig>,
    lower: Option<PermissionsConfig>,
) -> Option<PermissionsConfig> {
    let (ceiling, lower) = match (ceiling, lower) {
        (None, None) => return None,
        (Some(c), None) => return Some(c),
        (None, Some(l)) => return Some(l),
        (Some(c), Some(l)) => (c, l),
    };

    let commands = match (ceiling.commands, lower.commands) {
        (None, None) => None,
        (Some(c), None) => Some(c),
        (None, Some(l)) => Some(l),
        (Some(c), Some(l)) => {
            // Denies union: adding denies is narrowing, always legal.
            let mut deny = c.deny.clone();
            for pattern in l.deny {
                if !deny.contains(&pattern) {
                    deny.push(pattern);
                }
            }
            // Allows: the ceiling's non-empty allow bounds the lower
            // layer's — entries outside it are dropped here (and reported
            // as conflicts by the checker, not swallowed silently).
            let allow = if c.allow.is_empty() {
                l.allow
            } else if l.allow.is_empty() {
                c.allow
            } else {
                l.allow
                    .into_iter()
                    .filter(|pattern| c.allow.contains(pattern))
                    .collect()
            };
            Some(CommandPermissions { deny, allow })
        }
    };

    let packs = match (ceiling.packs, lower.packs) {
        (None, None) => None,
        (Some(c), None) => Some(c),
        (None, Some(l)) => Some(l),
        (Some(c), Some(l)) => {
            let executors = match (c.executors, l.executors) {
                (Some(a), Some(b)) => Some(if a.strictness() >= b.strictness() {
                    a
                } else {
                    b
                }),
                (a, b) => a.or(b),
            };
            let publishers = match (c.publishers, l.publishers) {
                (None, None) => None,
                (Some(p), None) => Some(p),
                (None, Some(p)) => Some(p),
                (Some(c_pub), Some(l_pub)) => {
                    // Empty = everyone — a non-empty ceiling bounds
                    // the lower list; both non-empty intersect.
                    let allow = if c_pub.allow.is_empty() {
                        l_pub.allow
                    } else if l_pub.allow.is_empty() {
                        c_pub.allow
                    } else {
                        l_pub
                            .allow
                            .into_iter()
                            .filter(|publisher| c_pub.allow.contains(publisher))
                            .collect()
                    };
                    Some(PublisherPermissions { allow })
                }
            };
            Some(PackPermissions {
                executors,
                publishers,
            })
        }
    };

    // Ceiling semantics, same as packs: the strictest declared wins.
    let scope_expansion = match (ceiling.scope_expansion, lower.scope_expansion) {
        (Some(c), Some(l)) => Some(if c.max_mode.strictness() >= l.max_mode.strictness() {
            c
        } else {
            l
        }),
        (c, l) => c.or(l),
    };

    let network = match (ceiling.network, lower.network) {
        (Some(c), Some(l)) => Some(NetworkPermissions {
            // `false` is the narrower value — a ceiling that turned the
            // default off stays off no matter what a lower layer says.
            default: c.default && l.default,
        }),
        (c, l) => c.or(l),
    };

    Some(PermissionsConfig {
        commands,
        packs,
        network,
        scope_expansion,
    })
}

/// Detects loosening attempts across ordered permission layers — the
/// case this guards is a repo layer trying to re-allow a pattern the org
/// layer denied: `check` rejects it, citing the offending layer.
/// `layers` come
/// ordered highest ceiling first (org, then user, then repo); every
/// returned string names the offending layer, the ceiling layer it
/// violated, and the exact pattern — comparison is textual on purpose:
/// mechanical and predictable, no cleverness about glob overlap.
pub fn permission_layer_conflicts(layers: &[(&str, &ConfigLayer)]) -> Vec<String> {
    let mut conflicts = Vec::new();

    for (lower_idx, (lower_name, lower_layer)) in layers.iter().enumerate() {
        let Some(lower) = &lower_layer.permissions else {
            continue;
        };
        for (higher_name, higher_layer) in layers.iter().take(lower_idx) {
            let Some(higher) = &higher_layer.permissions else {
                continue;
            };

            if let (Some(lower_cmds), Some(higher_cmds)) = (&lower.commands, &higher.commands) {
                for pattern in &lower_cmds.allow {
                    if higher_cmds.deny.contains(pattern) {
                        conflicts.push(format!(
                            "layer `{lower_name}` re-allows command pattern `{pattern}` denied by layer `{higher_name}` — permissions only narrow"
                        ));
                    } else if !higher_cmds.allow.is_empty() && !higher_cmds.allow.contains(pattern)
                    {
                        conflicts.push(format!(
                            "layer `{lower_name}` allows command pattern `{pattern}` outside layer `{higher_name}`'s allowlist — permissions only narrow"
                        ));
                    }
                }
            }

            if let (Some(lower_packs), Some(higher_packs)) = (&lower.packs, &higher.packs) {
                if let (Some(lower_pol), Some(higher_pol)) =
                    (lower_packs.executors, higher_packs.executors)
                {
                    if lower_pol.strictness() < higher_pol.strictness() {
                        conflicts.push(format!(
                            "layer `{lower_name}` loosens `packs.executors` to `{lower_pol:?}` below layer `{higher_name}`'s `{higher_pol:?}` — permissions only narrow"
                        ));
                    }
                }
                if let (Some(lower_pub), Some(higher_pub)) =
                    (&lower_packs.publishers, &higher_packs.publishers)
                {
                    if !higher_pub.allow.is_empty() {
                        for publisher in &lower_pub.allow {
                            if !higher_pub.allow.contains(publisher) {
                                conflicts.push(format!(
                                    "layer `{lower_name}` allows publisher `{publisher}` outside layer `{higher_name}`'s allowlist — permissions only narrow"
                                ));
                            }
                        }
                    }
                }
            }

            if let (Some(lower_net), Some(higher_net)) = (lower.network, higher.network) {
                if lower_net.default && !higher_net.default {
                    conflicts.push(format!(
                        "layer `{lower_name}` re-enables `network.default` turned off by layer `{higher_name}` — permissions only narrow"
                    ));
                }
            }

            if let (Some(lower_se), Some(higher_se)) =
                (lower.scope_expansion, higher.scope_expansion)
            {
                if lower_se.max_mode.strictness() < higher_se.max_mode.strictness() {
                    conflicts.push(format!(
                        "layer `{lower_name}` softens `scope_expansion.max_mode` to `{}` below layer `{higher_name}`'s `{}` — permissions only narrow",
                        lower_se.max_mode.as_str(),
                        higher_se.max_mode.as_str()
                    ));
                }
            }
        }
    }

    conflicts
}

fn merge_defaults_config(base: DefaultsConfig, more_specific: DefaultsConfig) -> DefaultsConfig {
    DefaultsConfig {
        isolation: more_specific.isolation.or(base.isolation),
        runner: more_specific.runner.or(base.runner),
        timeout_minutes: more_specific.timeout_minutes.or(base.timeout_minutes),
        on_failure: more_specific.on_failure.or(base.on_failure),
        max_parallel_nodes: more_specific.max_parallel_nodes.or(base.max_parallel_nodes),
        on_interrupt: more_specific.on_interrupt.or(base.on_interrupt),
    }
}

fn merge_limits_config(base: LimitsConfig, more_specific: LimitsConfig) -> LimitsConfig {
    LimitsConfig {
        max_tokens_per_run: more_specific.max_tokens_per_run.or(base.max_tokens_per_run),
        max_loop_iterations: more_specific
            .max_loop_iterations
            .or(base.max_loop_iterations),
        max_concurrent_runs: more_specific
            .max_concurrent_runs
            .or(base.max_concurrent_runs),
        max_workflow_depth: more_specific.max_workflow_depth.or(base.max_workflow_depth),
        max_expansion_files: more_specific
            .max_expansion_files
            .or(base.max_expansion_files),
        max_artifact_bytes: more_specific.max_artifact_bytes.or(base.max_artifact_bytes),
        inline_context_bytes: more_specific
            .inline_context_bytes
            .or(base.inline_context_bytes),
    }
}

/// A map whose values are arrays: a key
/// present in the more specific layer replaces the base's value for that
/// key wholesale, rather than concatenating the two arrays.
fn merge_map_replacing_values<K, V>(
    base: Option<BTreeMap<K, Vec<V>>>,
    more_specific: Option<BTreeMap<K, Vec<V>>>,
) -> Option<BTreeMap<K, Vec<V>>>
where
    K: Ord,
{
    merge_maps(base, more_specific, |_base_value, override_value| {
        override_value
    })
}

/// A map whose values are themselves mergeable structs (field by field).
fn merge_map_of_fields<K, V>(
    base: Option<BTreeMap<K, V>>,
    more_specific: Option<BTreeMap<K, V>>,
    merge_value: impl Fn(V, V) -> V,
) -> Option<BTreeMap<K, V>>
where
    K: Ord,
{
    merge_maps(base, more_specific, merge_value)
}

fn merge_maps<K, V>(
    base: Option<BTreeMap<K, V>>,
    more_specific: Option<BTreeMap<K, V>>,
    merge_value: impl Fn(V, V) -> V,
) -> Option<BTreeMap<K, V>>
where
    K: Ord,
{
    match (base, more_specific) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(more_specific)) => Some(more_specific),
        (Some(mut base), Some(more_specific)) => {
            for (key, value) in more_specific {
                let merged = match base.remove(&key) {
                    Some(base_value) => merge_value(base_value, value),
                    None => value,
                };
                base.insert(key, merged);
            }
            Some(base)
        }
    }
}

fn merge_fields<T>(
    base: Option<T>,
    more_specific: Option<T>,
    merge_value: impl Fn(T, T) -> T,
) -> Option<T> {
    match (base, more_specific) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(more_specific)) => Some(more_specific),
        (Some(base), Some(more_specific)) => Some(merge_value(base, more_specific)),
    }
}

fn merge_adapter_settings(
    base: AdapterSettings,
    more_specific: AdapterSettings,
) -> AdapterSettings {
    AdapterSettings {
        binary: more_specific.binary.or(base.binary),
        adapter_settings: more_specific.adapter_settings.or(base.adapter_settings),
    }
}

fn merge_storage_config(base: StorageConfig, more_specific: StorageConfig) -> StorageConfig {
    StorageConfig {
        path: more_specific.path.or(base.path),
        retention_days: more_specific.retention_days.or(base.retention_days),
    }
}

fn merge_project_config(base: ProjectConfig, more_specific: ProjectConfig) -> ProjectConfig {
    ProjectConfig {
        name: more_specific.name.or(base.name),
        base_branch: more_specific.base_branch.or(base.base_branch),
        branch_prefix: more_specific.branch_prefix.or(base.branch_prefix),
    }
}

fn merge_paths_config(base: PathsConfig, more_specific: PathsConfig) -> PathsConfig {
    PathsConfig {
        runs: more_specific.runs.or(base.runs),
        worktrees: more_specific.worktrees.or(base.worktrees),
    }
}

#[cfg(test)]
mod env_tests {
    use super::{user_state_root, Env};
    use std::path::PathBuf;

    #[test]
    fn yunta_home_overrides_home() {
        let env = Env {
            home: Some(PathBuf::from("/home/u")),
            yunta_home: Some(PathBuf::from("/scratch/state")),
            ..Default::default()
        };
        assert_eq!(user_state_root(&env), Some(PathBuf::from("/scratch/state")));
    }

    #[test]
    fn home_falls_back_to_dot_yunta() {
        let env = Env {
            home: Some(PathBuf::from("/home/u")),
            ..Default::default()
        };
        assert_eq!(user_state_root(&env), Some(PathBuf::from("/home/u/.yunta")));
    }

    #[test]
    fn neither_known_is_none() {
        assert_eq!(user_state_root(&Env::default()), None);
    }
}

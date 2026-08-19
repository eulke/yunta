//! Serde types for the workflow schema (T1.1) — **M-0 cut only**.
//!
//! The full T1.1 (M1) covers every node `kind`, `runners:`/`agent:`
//! resolution, `context:`, `skills:`, `modes:`, `inputs:`, gates and
//! composition. M-0's "schema recortado" (Plan, sección M-0) is
//! deliberately narrower: only `prompt`/`bash`/`loop` nodes, `depends_on`,
//! `scope`, `artifacts`, `hooks: {before, after}` and `on_failure.goto` —
//! just enough for the implement → compile → correct cycle the bootstrap
//! needs to validate. Every other workflow field is out of scope here and
//! will extend these types when its own task lands (T1.1 proper, in true
//! M1 execution), not before.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::NodeId;

/// A workflow definition (Contrato §2, §10 — minus modes/inputs, out of
/// scope for M-0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Workflow-level fallbacks a node inherits when it declares none of
    /// its own (T4.3, §11.1) — visible in the same file the team reads,
    /// never injected from a config layer (D81's own prohibition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_defaults: Option<NodeDefaults>,
    pub nodes: Vec<Node>,
}

/// `node_defaults:` (§11.1) — M-0/M4 cut: only `hooks`, the one consumer
/// T4.3 needs. Extends when another field needs the same "declare once,
/// nodes inherit" treatment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
}

/// A single node. Fields here are the ones the M-0 recorte names
/// explicitly; `runner` is included because a `prompt`/`loop` node is
/// meaningless without picking a runner, even though the resolution
/// mechanism itself (`runners:` candidates, capability probing, I17) is
/// config-layer work for a later task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Artifacts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<OnFailure>,
    /// How a `running` node with no terminal event is treated on resume
    /// after a crash (§8.1, D99, T4.5). Absent means the schema's own
    /// default (`restart_node`) applies, resolved the same way
    /// `defaults.isolation` is (config, then hardcoded default) — this
    /// field is the node's own override of that default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_interrupt: Option<OnInterrupt>,
    /// One-line summary `progress.md` shows for this node (§8.2, T5.5) —
    /// a node without one falls back to its own id. Not the same field as
    /// `Workflow.description` (that one's the whole workflow's own
    /// summary); §8.2 names this per-node without pointing at an existing
    /// schema key, so this is the schema addition it implies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The node's rung on the permissions ladder (§6.1, I18, T5.7): the
    /// session profile requested from the adapter for `prompt`/`loop`
    /// nodes. Absent means the engine's existing default (`edit`). Not
    /// to be confused with the config-level `permissions:` group — this
    /// scalar narrows what one node's agent session may do; that group
    /// governs which *commands* run at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<NodePermissions>,
    /// Declarative ONLY (§6.1, D105, T5.7): `network: false` activates no
    /// sandboxing — the engine never blocks a network call over it. It
    /// exists for policy and audit (a pack declaring it and then curling
    /// is a detectable contradiction, M11), and an executor may choose to
    /// actually enforce it on its own. Reading it as a sandbox is reading
    /// a guarantee the system never offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<bool>,
}

/// `permissions: read-only | edit | full` at node level (§6.1's ladder,
/// T5.7) — maps 1:1 onto the adapter's session profile. The names come
/// straight from the Contrato's own spelling.
/// `scope_expansion:` (§6.2, D73, T5.11) — governs how a loop's tasks may
/// grow past their own declared scope. `within` is a hard ceiling
/// ("jamás fuera de esto") checked in `rules` mode; `max_per_run` caps
/// how many expansions this run may grant before exhaustion escalates
/// ("diez concesiones seguidas no son readecuación, son un plan mal
/// cortado") — absent means uncapped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopeExpansion {
    #[serde(default)]
    pub mode: crate::events::ScopeExpansionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub within: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_run: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodePermissions {
    ReadOnly,
    Edit,
    Full,
}

/// A node's crash-recovery policy (§8.1, D99). `resume_session` (continue
/// the same agent conversation via its `session_id`, degrading to
/// `restart_node` with a warning when the adapter lacks the capability)
/// is real per the Contrato but has no consumer in this recorte — nothing
/// in `node_exec`'s dispatch path resumes a session on crash recovery
/// yet, only on an in-run retryable failure (T3.3), a different case. It
/// stays out of the enum rather than being accepted and silently ignored
/// — the same "no está diseñado, no entra al schema" treatment §7.3 gives
/// `isolation: container`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnInterrupt {
    #[default]
    RestartNode,
    FailIfUncertain,
}

/// The node kinds built so far (Plan, milestones M4/M5). `gate` and
/// `workflow` are the rest of the full T1.1 catalogue and stay out until
/// their own milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Prompt {
        prompt: PromptSource,
    },
    Bash {
        run: String,
    },
    Loop {
        until: String,
        prompt: PromptSource,
        /// Simultaneous `ready` tasks per batch (§5.5, D65) — absent
        /// means the engine's own default, `1` (sequential; "no hay caso
        /// especial", the batch mechanism handles both the same way).
        /// Declared per-node, deliberately: no config-level default
        /// exists anywhere in the reference schema, since token spend
        /// multiplies with it and nobody should discover that from the
        /// bill instead of the workflow file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concurrency: Option<u32>,
        /// §6.2/D73: the agent never widens its own scope — it requests,
        /// the engine (or a person, in `ask`) decides. Absent has the
        /// same effect as declaring it with no `mode:` — `deny` (§6.2's
        /// own default): every request becomes a finding, none are
        /// granted. Loop-scoped, not workflow- or config-scoped, because
        /// `scope_expansion_requested`'s own payload is keyed by
        /// `task_id` — this is ledger-task machinery, the same rung
        /// `concurrency:` already occupies on this node kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope_expansion: Option<ScopeExpansion>,
    },
    /// Nodes named by the author, run at once (§5.8, T4.6) — distinct
    /// from a loop's own `concurrency:` (§5.5), whose task count doesn't
    /// exist until the plan runs. Children are ordinary `Node`s (their
    /// own hooks/scope/artifacts/runner apply exactly as at the top
    /// level, T4.6 dispatches them through the same `execute_node`); they
    /// share the run's one worktree (T4.2 gives one per *run*, not per
    /// node) and aren't visible to the top-level DAG's own `depends_on`.
    Parallel {
        #[serde(default)]
        join: JoinPolicy,
        nodes: Vec<Node>,
    },
    /// Automatic verification against data the engine already has (§7.1,
    /// D85, T5.4) — never a person (that's `gate`, out of this recorte).
    /// The builtin list is closed on purpose: a `check` builtin is by
    /// definition something the engine can already evaluate; anything
    /// else is a `bash` node (exit code) or an `executor` (T5.6).
    Check {
        #[serde(flatten)]
        builtin: CheckBuiltin,
    },
    /// The extension point when neither `bash` (exit code only, no
    /// structured input) nor `check`'s closed builtin list covers it
    /// (§7.1/D85's own rationale for why this exists) — external code, a
    /// JSON contract over stdio (D47/D87, T5.6). `executor` names an
    /// entry in `skills.executors:`; `with` is opaque, executor-defined
    /// input. `D47`/`D87` fix the high-level shape (JSON in, JSON out,
    /// exit code is the verdict) but stop short of naming fields —
    /// `docs/m0-status.md`'s T5.6 entry documents the concrete contract
    /// this recorte adds on top, pending a real ADR revision.
    Executor {
        executor: String,
        #[serde(default)]
        with: serde_json::Map<String, serde_json::Value>,
        /// Absent means unenforced, same convention as `HookStep`'s own
        /// `timeout_seconds` — D47 only says "timeout del engine" without
        /// fixing a default or a field name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u64>,
    },
}

/// `kind: check`'s closed builtin list (§7.1). No budget builtin —
/// `limits:` already pauses the run on its own (§8.3); duplicating that
/// as a check would be redundant, per the Contrato's own text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "builtin", rename_all = "snake_case")]
pub enum CheckBuiltin {
    /// Re-runs `baseline.suite` and fails if something that passed the
    /// captured baseline stopped passing.
    BaselineCompare,
    /// Re-runs `coverage.cmd` and fails under `coverage.threshold`.
    CoverageGate,
    /// Fails if any finding posted so far in this run is at or above
    /// `max_severity`.
    FindingsGate {
        max_severity: crate::events::FindingSeverity,
    },
}

/// `parallel.join` (§5.8, D97). `all` (default): the group finishes only
/// once every child does, and one failed child fails the group. `any`:
/// the group finishes with the first child to *succeed*; the engine
/// sends the rest `interrupt`, escalating to `kill` if they don't close
/// in time (same ordered-then-forceful mechanism as an agent session's
/// own cancellation, Spec del Adapter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinPolicy {
    #[default]
    All,
    Any,
}

/// A node's prompt: an inline string, or `{file: ...}` (Contrato §9.3,
/// D78). The distinction is **structural, never heuristic**: a scalar is
/// always literal text, even if its contents look like a path — only an
/// explicit `{file: ...}` mapping reads from disk.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptSource {
    Inline(String),
    File(std::path::PathBuf),
}

impl<'de> Deserialize<'de> for PromptSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Scalar(String),
            File { file: std::path::PathBuf },
        }

        match Raw::deserialize(deserializer)? {
            Raw::Scalar(text) => Ok(PromptSource::Inline(text)),
            Raw::File { file } => Ok(PromptSource::File(file)),
        }
    }
}

impl Serialize for PromptSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            PromptSource::Inline(text) => serializer.serialize_str(text),
            PromptSource::File(path) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("file", path)?;
                map.end()
            }
        }
    }
}

/// `artifacts.produces` (Contrato §4). `task-ledger`, `findings` and
/// `questions` are interpreted (T5.1/T5.12/T5.14). A plain string stays
/// opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifacts {
    pub produces: Vec<ArtifactSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArtifactSpec {
    Plain(String),
    Typed { name: String, kind: ArtifactKind },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    TaskLedger,
    Findings,
    Questions,
}

/// `hooks: {before, after}` (D81, §11.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<HookStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<HookStep>,
}

/// One hook command. `timeout_seconds` is unenforced (no timeout) when
/// absent — additive over the pre-T4.3 engine, which never had one.
/// Field name/units aren't pinned by the Contrato's prose ("timeout corto
/// configurable"); seconds fit hook-scale commands better than the
/// minutes granularity `defaults.timeout_minutes` uses for whole agent
/// sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookStep {
    pub run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Distinct from a node's own `on_failure.goto` re-routing — a hook
    /// only ever fails or warns, never re-routes.
    #[serde(default, skip_serializing_if = "is_default_hook_failure_policy")]
    pub on_failure: HookFailurePolicy,
}

fn is_default_hook_failure_policy(policy: &HookFailurePolicy) -> bool {
    *policy == HookFailurePolicy::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    #[default]
    Fail,
    Warn,
}

/// `on_failure: {goto, max_reroutes}` — node-level re-routing (§11.2).
/// `max_reroutes` is mandatory (D24): a re-route without an explicit cap
/// is how a correction cycle turns infinite, so the schema refuses it.
/// Distinct from a hook's own `on_failure: fail|warn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnFailure {
    pub goto: NodeId,
    pub max_reroutes: u32,
}

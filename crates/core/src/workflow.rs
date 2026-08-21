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

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::NodeId;
use crate::inputs::InputSpec;

/// A workflow definition (Contrato §2, §10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `modes:` (§10.1, D44) — an ordered map, free in name and count:
    /// quick/standard/full are the reference workflows' own convention,
    /// never reserved schema words. **Declaration order is the
    /// promotion ladder** (§10.2) — promotion only ever targets a mode
    /// later in this map's own iteration order, never an earlier one.
    /// Absent entirely means the workflow has no modes at all: every
    /// node always runs, exactly pre-T9.1 behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<IndexMap<String, ModeSpec>>,
    /// `inputs:` (T1.5, §2.3, D82) — name is the map key, so the schema's
    /// own format guarantees uniqueness rather than a validation pass
    /// over a `[{name, ...}]` list. A `BTreeMap` rather than the
    /// declaration order: nothing in §2.3 or D82 gives that order any
    /// meaning (unlike `modes:`, whose declaration order *is* the
    /// promotion ladder, D44) — sorted iteration only makes catalog
    /// output (`list_workflows`, `--help`) and `check` diagnostics
    /// reproducible.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, InputSpec>,
    /// Workflow-level fallbacks a node inherits when it declares none of
    /// its own (T4.3, §11.1) — visible in the same file the team reads,
    /// never injected from a config layer (D81's own prohibition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_defaults: Option<NodeDefaults>,
    pub nodes: Vec<Node>,
    /// `yunta_schema: ">=1 <2"` (§2.1, DI-13) — a version range the
    /// binary's own schema major (`YUNTA_SCHEMA`) is checked against at
    /// `yunta check`, and frozen resolved into `run_created`. Absent
    /// means "whatever this binary speaks" (the reference text's own
    /// rule) — inferred, never an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    /// `on_finish:` (§8.3/D20, DI-13): close-of-run steps. The engine
    /// imposes the phase order (distill before any cleanup, §8.3) —
    /// declaration order in the YAML carries no meaning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_finish: Vec<OnFinishStep>,
}

impl Workflow {
    /// Every node in declaration (pre-)order, `parallel` children
    /// included — THE one owner of node traversal (DI-19): replay,
    /// progress, stats and check all derive per-node views from a flat
    /// `NodeId` map, so they all need exactly this walk and must never
    /// disagree about it. Sites that care about *structure* (a group
    /// and its children as a unit) still recurse on their own.
    pub fn iter_nodes(&self) -> NodeIter<'_> {
        NodeIter {
            stack: self.nodes.iter().rev().collect(),
        }
    }
}

/// See [`Workflow::iter_nodes`].
pub struct NodeIter<'a> {
    stack: Vec<&'a Node>,
}

impl<'a> Iterator for NodeIter<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<&'a Node> {
        let node = self.stack.pop()?;
        if let NodeKind::Parallel { nodes, .. } = &node.kind {
            self.stack.extend(nodes.iter().rev());
        }
        Some(node)
    }
}

/// One `on_finish:` entry — discriminated by its own field name, the
/// same untagged convention `context:` uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnFinishStep {
    Cleanup {
        cleanup: CleanupTarget,
    },
    /// Run artifacts the workflow declares durable (D20): distilled
    /// deterministically into `.yunta/knowledge/` at close (DI-24).
    Distill {
        distill: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupTarget {
    Worktree,
}

/// One `modes:` entry's own scope (§10.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeSpec {
    pub include: ModeInclude,
}

/// `include: all` or `include: [id, id, ...]` — the bare string literal
/// and a node-id sequence are the only two shapes §10.1's own example
/// shows, so this discriminates on the YAML value's shape rather than
/// adding a `kind:` key neither form has.
///
/// Both directions are hand-written, deliberately paired: `derive`'s
/// default untagged-enum behavior would serialize the unit variant
/// `All` as `null`, not the string `"all"` the custom `Deserialize`
/// below expects back — a manifest round-trip (write, then a later
/// `status`/`resume` reading it back) would otherwise silently break on
/// its own output.
#[derive(Debug, Clone, PartialEq)]
pub enum ModeInclude {
    All,
    Nodes(Vec<NodeId>),
}

impl Serialize for ModeInclude {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ModeInclude::All => serializer.serialize_str("all"),
            ModeInclude::Nodes(nodes) => nodes.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ModeInclude {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            All(AllLiteral),
            Nodes(Vec<NodeId>),
        }
        #[derive(Deserialize)]
        enum AllLiteral {
            #[serde(rename = "all")]
            All,
        }
        match Raw::deserialize(deserializer)? {
            Raw::All(AllLiteral::All) => Ok(ModeInclude::All),
            Raw::Nodes(nodes) => Ok(ModeInclude::Nodes(nodes)),
        }
    }
}

/// `node_defaults:` (§11.1) — M-0/M4 cut: only `hooks`, the one consumer
/// T4.3 needs. Extends when another field needs the same "declare once,
/// nodes inherit" treatment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
    /// Skills every node mounts unless it declares its own list (DI-13)
    /// — same replace-wholesale inheritance as `hooks`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
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
    /// `runners: [role, role]` (§13.2, T9.4) — static fan-out: the
    /// manifest expands this node into one `<id>@<role>` node per role
    /// before anything runs. Mutually exclusive with `runner:` (check).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runners: Vec<String>,
    /// `agent:` at node level (§13.3, D37) — overrides the resolved
    /// candidate's own agent for this node. Portable field: each
    /// adapter maps it to its native mechanism, and one without
    /// `custom_agents` fails the node rather than silently ignoring it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
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
    /// `context:` (§9, T6.1/DI-17) — data resolved and materialized
    /// *before* a session opens, in declaration order. Consumed by
    /// `kind: prompt` (the node's one session) and `kind: loop` (once
    /// per task brief, volatile sources fresh and stable ones memoized
    /// per §9.1's classes); `check` rejects it on any kind that opens
    /// no session (`bash`/`check`/`executor`/`gate`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextSpec>,
    /// `skills: [names]` (D47, DI-13) — instructions and capabilities
    /// the adapter mounts by its native mechanism: *how* to work, where
    /// `context:` injects *what* to work on. Resolved against
    /// `skills.paths` (repo first); an adapter with no native mechanism
    /// degrades with `capability_degraded`, never a fatal error (A6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// `interactive: true` (§4.1, DI-02/DI-13) — presentation datum for
    /// this node's questions: the surface renders them as a live
    /// conversation when it can. With no surface, nothing changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<bool>,
    /// `fresh_context:` (§8.2, DI-13) — `true` (and absent) means this
    /// node's session opens fresh, pure rehydration from the log.
    /// `false` requires session resume (DI-23), which isn't built:
    /// `check` refuses it with an actionable error instead of accepting
    /// it silently (A6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_context: Option<bool>,
    /// `invariant: true` (§10.1, D44) — this node's verification/scope/
    /// baseline/hygiene role is non-negotiable: every declared mode must
    /// include it, checked independent of any mode's name or count. A
    /// mode narrows deliberation, never verification.
    #[serde(default)]
    pub invariant: bool,
}

/// One `context:` entry (§9): a builtin `ContextSource` plus its own
/// parameters. Untagged: each variant's own (unique) field name is the
/// discriminant, exactly matching the Contrato's own YAML — `- files:
/// [...]`, `- command: "..."`, `- artifact: { node: ..., name: ... }`,
/// and so on; there is no separate `kind:` key to introduce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextSpec {
    Files {
        files: Vec<String>,
    },
    Command {
        command: String,
    },
    Artifact {
        artifact: ArtifactContextRef,
    },
    Mcp {
        mcp: McpQueryParams,
    },
    RunEvents {
        #[serde(rename = "run-events")]
        run_events: RunEventsParams,
    },
    Ledger {
        ledger: LedgerParams,
    },
    Knowledge {
        knowledge: KnowledgeParams,
    },
    NodeOutput {
        #[serde(rename = "node-output")]
        node_output: NodeOutputParams,
    },
}

/// `mcp: { server: ..., query: ... }` (§9, T6.2) — `server` names an
/// entry in the merged config's `mcp_servers:`; `query` is free-form text
/// (the Contrato's only example passes `{{inputs.idea}}` verbatim) sent
/// to the server as the resolver's own choice of MCP call (T6.2:
/// `tools/call` on a tool literally named `query`, since the Contrato
/// fixes neither the MCP verb nor a tool name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpQueryParams {
    pub server: String,
    pub query: String,
}

/// `artifact: { node: ..., name: ... }` (§9) — the referenced node's own
/// declared artifact. Reading it creates an *implicit* `depends_on` edge
/// (`build_manifest` expands it into the frozen workflow's own
/// `depends_on`, so `check`/the scheduler need no separate awareness of
/// `context:` at all — by the time either runs, the edge is already
/// ordinary `depends_on`).
///
/// `node` is optional (D108): `artifact: { name }` means "an artifact of
/// this run's dir, whoever produced it" — a mounted one included. It
/// creates no implicit edge (there is no producer to order behind), and
/// it's what keeps a catalog child parametric: it never has to name a
/// producer it doesn't have.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactContextRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub name: String,
}

/// `run-events: { filter: ... }` (§9) — a read-only query into the run's
/// own event log. `filter` stays a free-form string (the Contrato's only
/// example is `filter: failed`, no closed vocabulary given) — the
/// resolver's own job (T6.1) to interpret, not the schema's.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RunEventsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// `ledger: {}` (§9) — no parameters in this recorte's own resolution
/// (the aggregate ledger/task-status view; see the node's own doc
/// comment on `context` for the task-scoped variant this doesn't cover
/// yet).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LedgerParams {}

/// One layer of `knowledge:` (§9.2), most to least local. `Org` resolves
/// as the union of every installed knowledge pack's declared contents
/// (RFC-0002 vendoring; D109/DI-31) — a same-filename collision between
/// two packs is a typed error at resolution time, since between packs
/// there is no precedence to fall back on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeLayer {
    Repo,
    User,
    Org,
}

impl std::fmt::Display for KnowledgeLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnowledgeLayer::Repo => write!(f, "repo"),
            KnowledgeLayer::User => write!(f, "user"),
            KnowledgeLayer::Org => write!(f, "org"),
        }
    }
}

/// `knowledge: { layers: [...] }` (§9.2) — empty/absent `layers` means
/// every layer the resolver can see.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct KnowledgeParams {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<KnowledgeLayer>,
}

/// `node-output: { node: ... }` (§9, §11.2) — captured stdout/stderr of a
/// previously-run node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeOutputParams {
    pub node: NodeId,
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

/// A node's crash-recovery policy (§8.1, D99) — the full Contrato
/// triple since DI-23.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnInterrupt {
    #[default]
    RestartNode,
    FailIfUncertain,
    /// DI-23/D99: continue the same agent conversation — the
    /// `session_id` the log recorded (`agent_session_opened`, DI-09) is
    /// handed back to the adapter's `resume`. Only `kind: prompt` opens
    /// a node-scoped session, so `check` refuses the explicit
    /// declaration anywhere else; an adapter without the
    /// `resume_session` capability — or a crash before any session
    /// opened — degrades to `restart_node` with an explicit
    /// `capability_degraded` event (the Contrato's own "degrada con
    /// warning"), never silently. As a *config default*
    /// (`defaults.on_interrupt`) it applies where a session exists;
    /// kinds without one (bash/check/…, and a loop's per-task sessions)
    /// restart, which is the only meaning the policy can have there.
    ResumeSession,
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
        /// `coordination:` (§6.4, D49/D98, T8.2) — whether this group's
        /// children share a blackboard. `independent` (default): the
        /// blackboard tools are never even mounted — the right shape
        /// for evaluative groups (reviewers), where cross-contamination
        /// anchors judgments and kills the diversity the fan-out buys.
        /// `blackboard`: children with `run_tools` get
        /// `yunta_post_finding`/`yunta_get_blackboard` scoped to this
        /// group (never the whole run); reading siblings' posts still
        /// waits for the `join` (D98).
        #[serde(default)]
        coordination: Coordination,
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
    /// A human decision resolved outside this process (§5.6, D66) — v1's
    /// only shape is `external: {kind: pull_request, ...}`: the engine
    /// delegates the multi-person substrate (identity, permissions,
    /// notifications) to the team's forge instead of building `serve`
    /// early. With `external: None` this is an **internal gate**
    /// (DI-04) — the shape the reference workflows' own `approve-plan`/
    /// `ship` use and T1.3's mode-coherence rule ("opción de gate")
    /// requires: resolved through `HumanInteraction` (console today,
    /// M8's `resolve_gate` later) with the declared `options`, and
    /// `on:` mapping an option to a §11.2-style re-route.
    Gate {
        /// Who the escalation names — mirrors §5.3's own audience
        /// concept: a human on the forge (external), or whoever holds
        /// the interactive surface (internal).
        assignee: String,
        /// The question the internal gate asks — becomes the §5.3
        /// object's `summary`. Absent, a default derived from the node
        /// id is used.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// Declared choices, free ids (reference: `[aprobar, ajustar,
        /// abortar]`). Empty means the single default option `approve`;
        /// the engine always appends its own `abort` (§5.3: aborting is
        /// always a valid exit, same convention T7.2's escalation uses).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
        /// Option → re-route target (`on: { ajustar: plan }`): choosing
        /// a mapped option re-routes exactly like `on_failure.goto`
        /// (§11.2) — the target and its subgraph complete, then the
        /// gate returns to ready and asks again. Unbounded on purpose:
        /// each lap is human-driven, not an automatic cycle
        /// `max_reroutes` exists to cap. An unmapped option resolves
        /// the gate and the DAG continues.
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        on: IndexMap<String, NodeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        external: Option<ExternalGate>,
    },
    /// `kind: workflow` (§12, T9.3): runs another workflow as a
    /// sub-run — a **complete run** with its own run_id, manifest, event
    /// log and run.dir, never an inline expansion. The parent freezes
    /// only the child's *name and inputs* (these two fields); the child
    /// resolves and freezes its own workflow file at birth, so a long
    /// process picks up improvements to child workflows between
    /// executions without any individual run losing immutability.
    Workflow {
        /// The child workflow's name, resolved at child birth against
        /// the repo catalog: `.yunta/workflows/<name>.yaml` in the
        /// parent run's own working tree (versioned, the same catalog
        /// `list_workflows` reads).
        r#use: String,
        /// Inputs handed to the child, template-rendered in the
        /// parent's own scope (`{{inputs.x}}`, `{{run.branch}}`, …)
        /// before the child validates them against its declared
        /// `inputs:`. Absent means the child must get by on defaults.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        inputs: BTreeMap<String, String>,
        /// `worktree` (default) gives the child its own tree branched
        /// off the parent's HEAD; `inherit` shares the parent's tree
        /// for phases of one piece of work — parallel `inherit`
        /// siblings must declare disjoint `scope` (checked).
        #[serde(default, skip_serializing_if = "is_default_workflow_isolation")]
        isolation: WorkflowIsolation,
        /// `mounts:` (§12, D108) — artifacts of the parent's own graph
        /// copied into the child's `run.dir/artifacts/` at birth: the
        /// promotion inheritance mechanism generalized ("la promoción es
        /// un caso particular de este mecanismo general"). The parent
        /// declares because the parent is who knows its own topology — a
        /// catalog child naming a sibling would be welded to one
        /// parent's shape and lose its standalone run. Each mount
        /// implies `depends_on` on the referenced node, which is what
        /// guarantees §12's "hermanos terminados".
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mounts: Vec<MountSpec>,
    },
}

/// One `mounts:` entry — `artifact:` is the only mount source there is,
/// kept as a named field (not a bare inline struct) so a second source
/// kind lands as a sibling field with the same untagged-by-field-name
/// convention `context:`/`on_finish:` already use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountSpec {
    pub artifact: MountArtifact,
}

/// The mounted artifact: `node` names a node of the *parent's own*
/// graph — a `kind: workflow` sibling resolves through the recorded
/// link (`child_run_finished`) to that child run's artifacts, any other
/// node to the parent's own `run.dir/artifacts/`. `as:` renames the
/// copy in the child (absent keeps `name`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountArtifact {
    pub node: NodeId,
    pub name: String,
    #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
    pub rename: Option<String>,
}

fn is_default_workflow_isolation(isolation: &WorkflowIsolation) -> bool {
    *isolation == WorkflowIsolation::default()
}

/// A `kind: workflow` node's `isolation:` (§12) — deliberately its own
/// enum, not [`crate::Isolation`]: `inherit` only exists for workflow
/// nodes ("`inherit` solo en nodos workflow", the reference config's own
/// comment), and a run-level `none` is not a per-node choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowIsolation {
    #[default]
    Worktree,
    Inherit,
}

/// `kind: gate`'s `external:` block (§5.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalGate {
    pub kind: ForgeKind,
    /// Paths (relative to `run.dir`) committed to `branch` for review —
    /// §5.6's own example: `[spec.md]`.
    pub artifacts: Vec<String>,
    /// Template-rendered branch name the artifacts are pushed to and the
    /// PR is opened from (`{{run.branch}}`, §5.6's own example, resolves
    /// to `yunta/<run_id>` — a fresh push target, not necessarily the
    /// worktree's own local checkout branch, since `isolation: none`
    /// never creates one).
    pub branch: String,
}

/// The only forge integration v1 has — kept as a closed enum (not a
/// bare string) so a second one lands as a new variant with exhaustive
/// match-checking everywhere it matters, the same reasoning
/// `CheckBuiltin`'s own closed list uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeKind {
    PullRequest,
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

/// `parallel.coordination` (§6.4, D49) — see the field's own doc on
/// [`NodeKind::Parallel`]. A closed enum, not a bool: a third
/// coordination shape (if one ever earns an ADR) lands as a variant
/// with exhaustive match-checking, same reasoning as `JoinPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coordination {
    #[default]
    Independent,
    Blackboard,
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

//! The workflow schema: what `.yunta/workflows/<name>.yaml` declares,
//! parsed once at the frontier.
//!
//! A person writes these files, so every type here refuses a key it
//! does not know and names the keys it does. The values the schema
//! discriminates by a field name instead of a tag — a `context:` entry,
//! an `on_finish:` step, an artifact, a prompt, a mode's `include` —
//! read that name explicitly and say which names exist when it is
//! missing or wrong, instead of reporting that nothing matched.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::{AgentName, ExecutorName, ModeName, NodeId, RunnerName};
use crate::inputs::InputSpec;
use crate::yaml::{self, Mapping, Value, YamlError};

/// A workflow definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `modes:` — an ordered map, free in name and count:
    /// quick/standard/full are the reference workflows' own convention,
    /// never reserved schema words. **Declaration order is the
    /// promotion ladder** — promotion only ever targets a mode
    /// later in this map's own iteration order, never an earlier one.
    /// Absent entirely means the workflow has no modes at all: every
    /// node always runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<IndexMap<ModeName, ModeSpec>>,
    /// `inputs:` — name is the map key, so the schema's
    /// own format guarantees uniqueness rather than a validation pass
    /// over a `[{name, ...}]` list. A `BTreeMap` rather than the
    /// declaration order: nothing about inputs gives that order any
    /// meaning (unlike `modes:`, whose declaration order *is* the
    /// promotion ladder) — sorted iteration only makes catalog
    /// output (`list_workflows`, `--help`) and `check` diagnostics
    /// reproducible.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, InputSpec>,
    /// Workflow-level fallbacks a node inherits when it declares none of
    /// its own — visible in the same file the team reads,
    /// never injected from a config layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_defaults: Option<NodeDefaults>,
    pub nodes: Vec<Node>,
    /// `yunta_schema: ">=1 <2"` — a version range the
    /// binary's own schema major (`YUNTA_SCHEMA`) is checked against at
    /// `yunta check`, and frozen resolved into `run_created`. Absent
    /// means "whatever this binary speaks" (the reference text's own
    /// rule) — inferred, never an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    /// `on_finish:`: close-of-run steps. The engine
    /// imposes the phase order (distill before any cleanup) —
    /// declaration order in the YAML carries no meaning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_finish: Vec<OnFinishStep>,
}

impl Workflow {
    /// Every node in declaration (pre-)order, `parallel` children
    /// included — THE one owner of node traversal: replay,
    /// progress, stats and check all derive per-node views from a flat
    /// `NodeId` map, so they all need exactly this walk and must never
    /// disagree about it. Sites that care about *structure* (a group
    /// and its children as a unit) still recurse on their own.
    pub fn iter_nodes(&self) -> NodeIter<'_> {
        NodeIter {
            stack: self.nodes.iter().rev().collect(),
        }
    }

    /// Every node in the same order as [`iter_nodes`](Self::iter_nodes),
    /// each paired with its enclosing `parallel` group node — `Some(group)`
    /// for a group's child, `None` for a top-level node. The one walk for a
    /// check that must attribute a child's finding to its group, or reason
    /// about a node together with the siblings it shares a worktree with.
    pub fn iter_nodes_with_group(&self) -> NodeGroupIter<'_> {
        NodeGroupIter {
            stack: self.nodes.iter().rev().map(|node| (node, None)).collect(),
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

/// See [`Workflow::iter_nodes_with_group`].
pub struct NodeGroupIter<'a> {
    stack: Vec<(&'a Node, Option<&'a Node>)>,
}

impl<'a> Iterator for NodeGroupIter<'a> {
    type Item = (&'a Node, Option<&'a Node>);

    fn next(&mut self) -> Option<Self::Item> {
        let (node, group) = self.stack.pop()?;
        if let NodeKind::Parallel { nodes, .. } = &node.kind {
            self.stack
                .extend(nodes.iter().rev().map(|child| (child, Some(node))));
        }
        Some((node, group))
    }
}

/// One `on_finish:` entry — discriminated by its own field name, the
/// same convention `context:` uses.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum OnFinishStep {
    Cleanup {
        cleanup: CleanupTarget,
    },
    /// Run artifacts the workflow declares durable: distilled
    /// deterministically into `.yunta/knowledge/` at close.
    Distill {
        distill: Vec<String>,
    },
}

impl OnFinishStep {
    const KEYS: &'static [&'static str] = &["cleanup", "distill"];
}

impl<'de> Deserialize<'de> for OnFinishStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (key, value) = keyed_entry(deserializer, "an `on_finish` step", Self::KEYS)?;
        match key.as_str() {
            "cleanup" => Ok(OnFinishStep::Cleanup {
                cleanup: nested::<D, _>(&key, value)?,
            }),
            _ => Ok(OnFinishStep::Distill {
                distill: nested::<D, _>(&key, value)?,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CleanupTarget {
    Worktree,
}

/// One `modes:` entry's own scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModeSpec {
    pub include: ModeInclude,
}

/// `include: all` or `include: [id, id, ...]` — the bare string literal
/// and a node-id sequence are the only two shapes the schema
/// allows, so this discriminates on the YAML value's shape rather than
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
        use serde::de::Error;
        match Value::deserialize(deserializer)? {
            Value::String(word) if word == "all" => Ok(ModeInclude::All),
            sequence @ Value::Sequence(_) => {
                Ok(ModeInclude::Nodes(nested::<D, _>("include", sequence)?))
            }
            other => Err(D::Error::custom(format!(
                "`include` is `all` or a list of node ids, not {}",
                describe(&other)
            ))),
        }
    }
}

impl schemars::JsonSchema for ModeInclude {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ModeInclude".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "`all`, or the ids of the nodes the mode includes",
            "oneOf": [
                { "const": "all" },
                { "type": "array", "items": generator.subschema_for::<NodeId>() }
            ]
        })
    }
}

/// `node_defaults:` — currently carries only `hooks`, the one consumer
/// needs. Extends when another field needs the same "declare once,
/// nodes inherit" treatment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
    /// Skills every node mounts unless it declares its own list
    /// — same replace-wholesale inheritance as `hooks`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
}

/// A single node. Fields here are the ones currently implemented;
/// `runner` is included because a `prompt`/`loop` node is
/// meaningless without picking a runner, even though the resolution
/// mechanism itself (`runners:` candidates, capability probing) is
/// config-layer work handled elsewhere.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Node {
    pub id: NodeId,
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<RunnerName>,
    /// `runners: [name, name]` — static fan-out: the manifest expands
    /// this node into one `<id>@<runner>` node per runner before
    /// anything runs. Mutually exclusive with `runner:` (check).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runners: Vec<RunnerName>,
    /// `agent:` at node level — overrides the resolved
    /// candidate's own agent for this node. Portable field: each
    /// adapter maps it to its native mechanism, and one without
    /// `custom_agents` fails the node rather than silently ignoring it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Artifacts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Hooks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<OnFailure>,
    /// How a `running` node with no terminal event is treated on resume
    /// after a crash. Absent means the schema's own
    /// default (`restart_node`) applies, resolved the same way
    /// `defaults.isolation` is (config, then hardcoded default) — this
    /// field is the node's own override of that default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_interrupt: Option<OnInterrupt>,
    /// One-line summary `progress.md` shows for this node —
    /// a node without one falls back to its own id. Not the same field as
    /// `Workflow.description` (that one's the whole workflow's own
    /// summary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The node's rung on the permissions ladder: the
    /// session profile requested from the adapter for `prompt`/`loop`
    /// nodes. Absent means the engine's existing default (`edit`). Not
    /// to be confused with the config-level `permissions:` group — this
    /// scalar narrows what one node's agent session may do; that group
    /// governs which *commands* run at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<NodePermissions>,
    /// A node's network policy, declarative ONLY: `network: false` asks for
    /// no network access, `network: true` allows it, and absent (`None`)
    /// declares no policy at all. The engine never blocks a network call
    /// over this — it exists for policy and audit (a pack declaring `false`
    /// and then curling is a detectable contradiction), and an executor may
    /// choose to enforce it on its own. Where a session's resolved adapter
    /// cannot enforce a declared `network: false`, the engine records a
    /// `capability_degraded` before the session; reading it as a sandbox is
    /// reading a guarantee the system never offered (D105/D119).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<bool>,
    /// `context:` — data resolved and materialized
    /// *before* a session opens, in declaration order. Consumed by
    /// `kind: prompt` (the node's one session) and `kind: loop` (once
    /// per task brief, volatile sources fresh and stable ones memoized
    /// per their stability class); `check` rejects it on any kind that opens
    /// no session (`bash`/`check`/`executor`/`gate`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextSpec>,
    /// `skills: [names]` — instructions and capabilities
    /// the adapter mounts by its native mechanism: *how* to work, where
    /// `context:` injects *what* to work on. Resolved against
    /// `skills.paths` (repo first); an adapter with no native mechanism
    /// degrades with `capability_degraded`, never a fatal error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// `interactive: true` — presentation datum for
    /// this node's questions: the surface renders them as a live
    /// conversation when it can. With no surface, nothing changes.
    /// Absent means `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub interactive: bool,
    /// `invariant: true` — this node's verification/scope/
    /// baseline/hygiene role is non-negotiable: every declared mode must
    /// include it, checked independent of any mode's name or count. A
    /// mode narrows deliberation, never verification.
    #[serde(default)]
    pub invariant: bool,
}

/// The keys every node accepts at its own level; `kind` and the keys
/// that belong to the kind are [`NodeKind`]'s.
const NODE_KEYS: &[&str] = &[
    "id",
    "depends_on",
    "scope",
    "runner",
    "runners",
    "agent",
    "artifacts",
    "hooks",
    "on_failure",
    "on_interrupt",
    "description",
    "permissions",
    "network",
    "context",
    "skills",
    "interactive",
    "invariant",
];

/// Keys an author reaches for that no node accepts, each with the key
/// that expresses the intent.
const RETIRED_NODE_KEYS: &[(&str, &str)] = &[
    ("role", "a node names its runner with `runner:`"),
    (
        "fresh_context",
        "every session starts fresh; `on_interrupt: resume_session` reuses one only when a \
         run resumes",
    ),
];

/// The node-level keys as a struct of their own — what [`Node`]'s
/// deserializer reads once the kind's keys are split off.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct NodeFields {
    id: NodeId,
    #[serde(default)]
    depends_on: Vec<NodeId>,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    runner: Option<RunnerName>,
    #[serde(default)]
    runners: Vec<RunnerName>,
    #[serde(default)]
    agent: Option<AgentName>,
    #[serde(default)]
    artifacts: Option<Artifacts>,
    #[serde(default)]
    hooks: Option<Hooks>,
    #[serde(default)]
    on_failure: Option<OnFailure>,
    #[serde(default)]
    on_interrupt: Option<OnInterrupt>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    permissions: Option<NodePermissions>,
    #[serde(default)]
    network: Option<bool>,
    #[serde(default)]
    context: Vec<ContextSpec>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    interactive: bool,
    #[serde(default)]
    invariant: bool,
}

impl<'de> Deserialize<'de> for Node {
    /// A node is one mapping holding its own keys and its kind's keys
    /// side by side. The keys are split by name before either part is
    /// parsed, so every unknown key is reported at once — with the kind
    /// it was judged against, the keys that are valid there and, for a
    /// key the schema retired, the key that replaces it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let mut own = Mapping::new();
        let mut kind_part = Mapping::new();
        for (key, value) in Mapping::deserialize(deserializer)? {
            let Some(name) = key.as_str() else {
                return Err(D::Error::custom(format!(
                    "a node's keys are strings, not {}",
                    describe(&key)
                )));
            };
            if NODE_KEYS.contains(&name) {
                own.insert(key, value);
            } else {
                kind_part.insert(key, value);
            }
        }
        let subject = match own.get("id").and_then(Value::as_str) {
            Some(id) => format!("node `{id}`"),
            None => "node".to_string(),
        };

        let Some(kind_name) = kind_part.get("kind").and_then(Value::as_str) else {
            return Err(D::Error::custom(format!(
                "{subject}: `kind` is missing or not a string; one of {}",
                list(NodeKind::KINDS)
            )));
        };
        let Some(kind_keys) = NodeKind::keys(kind_name) else {
            return Err(D::Error::custom(format!(
                "{subject}: unknown kind `{kind_name}`; one of {}",
                list(NodeKind::KINDS)
            )));
        };
        let unknown: Vec<&str> = kind_part
            .iter()
            .filter_map(|(key, _)| key.as_str())
            .filter(|key| *key != "kind" && !kind_keys.contains(key))
            .collect();
        if !unknown.is_empty() {
            let valid: Vec<&str> = NODE_KEYS
                .iter()
                .chain(std::iter::once(&"kind"))
                .chain(kind_keys.iter())
                .copied()
                .collect();
            let mut message = format!(
                "{subject}: unknown key(s) {} for a `{kind_name}` node; valid keys: {}",
                list(&unknown),
                list(&valid)
            );
            for (key, hint) in RETIRED_NODE_KEYS {
                if unknown.contains(key) {
                    message.push_str(&format!("; `{key}`: {hint}"));
                }
            }
            return Err(D::Error::custom(message));
        }

        let fields: NodeFields = yaml::from_value(Value::Mapping(own))
            .map_err(|error| D::Error::custom(format!("{subject}: {error}")))?;
        if fields.id.is_fan_out() {
            return Err(D::Error::custom(format!(
                "{subject}: `@` is reserved for the fan-out siblings the manifest expands \
                 `runners:` into; an authored id is a letter followed by letters, digits, `_` \
                 or `-`"
            )));
        }
        let kind: NodeKind = yaml::from_value(Value::Mapping(kind_part))
            .map_err(|error| D::Error::custom(format!("{subject}: {error}")))?;
        Ok(Node {
            id: fields.id,
            kind,
            depends_on: fields.depends_on,
            scope: fields.scope,
            runner: fields.runner,
            runners: fields.runners,
            agent: fields.agent,
            artifacts: fields.artifacts,
            hooks: fields.hooks,
            on_failure: fields.on_failure,
            on_interrupt: fields.on_interrupt,
            description: fields.description,
            permissions: fields.permissions,
            network: fields.network,
            context: fields.context,
            skills: fields.skills,
            interactive: fields.interactive,
            invariant: fields.invariant,
        })
    }
}

/// One `context:` entry: a builtin `ContextSource` plus its own
/// parameters, discriminated by its own field name, exactly matching
/// the schema's YAML — `- files: [...]`, `- command: "..."`,
/// `- artifact: { node: ..., name: ... }`, and so on; there is no
/// separate `kind:` key to introduce.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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

impl ContextSpec {
    const KEYS: &'static [&'static str] = &[
        "files",
        "command",
        "artifact",
        "mcp",
        "run-events",
        "ledger",
        "knowledge",
        "node-output",
    ];
}

impl<'de> Deserialize<'de> for ContextSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (key, value) = keyed_entry(deserializer, "a context source", Self::KEYS)?;
        let spec = match key.as_str() {
            "files" => ContextSpec::Files {
                files: nested::<D, _>(&key, value)?,
            },
            "command" => ContextSpec::Command {
                command: nested::<D, _>(&key, value)?,
            },
            "artifact" => ContextSpec::Artifact {
                artifact: nested::<D, _>(&key, value)?,
            },
            "mcp" => ContextSpec::Mcp {
                mcp: nested::<D, _>(&key, value)?,
            },
            "run-events" => ContextSpec::RunEvents {
                run_events: nested::<D, _>(&key, value)?,
            },
            "ledger" => ContextSpec::Ledger {
                ledger: nested::<D, _>(&key, value)?,
            },
            "knowledge" => ContextSpec::Knowledge {
                knowledge: nested::<D, _>(&key, value)?,
            },
            _ => ContextSpec::NodeOutput {
                node_output: nested::<D, _>(&key, value)?,
            },
        };
        Ok(spec)
    }
}

/// `mcp: { server: ..., query: ... }` — `server` names an
/// entry in the merged config's `mcp_servers:`; `query` is free-form text
/// (the reference example passes `{{inputs.idea}}` verbatim) sent
/// to the server as the resolver's own choice of MCP call (currently
/// `tools/call` on a tool literally named `query`, since the schema
/// fixes neither the MCP verb nor a tool name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpQueryParams {
    pub server: String,
    pub query: String,
}

/// `artifact: { node: ..., name: ... }` — the referenced node's own
/// declared artifact. Reading it creates an *implicit* `depends_on` edge
/// (`build_manifest` expands it into the frozen workflow's own
/// `depends_on`, so `check`/the scheduler need no separate awareness of
/// `context:` at all — by the time either runs, the edge is already
/// ordinary `depends_on`).
///
/// `node` is optional: `artifact: { name }` means "an artifact of
/// this run's dir, whoever produced it" — a mounted one included. It
/// creates no implicit edge (there is no producer to order behind), and
/// it's what keeps a catalog child parametric: it never has to name a
/// producer it doesn't have.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactContextRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub name: String,
}

/// `run-events: { filter: ... }` — a read-only query into the run's
/// own event log. `filter` is a closed vocabulary parsed to
/// [`RunEventsFilter`], so an unknown value is rejected when the
/// workflow is read (and `yunta check` surfaces it), never carried to
/// the resolver as a string it must reject at runtime.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunEventsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<RunEventsFilter>,
}

/// The `run-events` filters the resolver knows. Absent (`None`) means
/// the whole log; `Failed` narrows it to `node_failed` events, `Findings`
/// to `finding_posted` events (what a corrective node reads to act on
/// what an earlier node found).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunEventsFilter {
    Failed,
    Findings,
}

impl RunEventsFilter {
    /// The YAML spelling, for diagnostics and source labels.
    pub fn as_str(self) -> &'static str {
        match self {
            RunEventsFilter::Failed => "failed",
            RunEventsFilter::Findings => "findings",
        }
    }
}

/// `ledger: {}` — no parameters in the current resolution
/// (the aggregate ledger/task-status view; see the node's own doc
/// comment on `context` for the task-scoped variant this doesn't cover
/// yet).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LedgerParams {}

/// One layer of `knowledge:`, most to least local. `Org` resolves
/// as the union of every installed knowledge pack's declared contents
/// (from pack vendoring) — a same-filename collision between
/// two packs is a typed error at resolution time, since between packs
/// there is no precedence to fall back on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// `knowledge: { layers: [...] }` — empty/absent `layers` means
/// every layer the resolver can see.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeParams {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<KnowledgeLayer>,
}

/// `node-output: { node: ... }` — captured stdout/stderr of a
/// previously-run node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeOutputParams {
    pub node: NodeId,
}

/// `permissions: read-only | edit | full` at node level — maps 1:1 onto
/// the adapter's session profile. The names come
/// straight from the reference schema's own spelling.
/// `scope_expansion:` — governs how a loop's tasks may
/// grow past their own declared scope. `within` is a hard ceiling
/// ("never outside this") checked in `rules` mode; `max_per_run` caps
/// how many expansions this run may grant before exhaustion escalates
/// ("ten grants in a row are not re-scoping, they are a badly-cut
/// plan") — absent means uncapped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeExpansion {
    #[serde(default)]
    pub mode: crate::policy::ScopeExpansionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub within: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_run: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NodePermissions {
    ReadOnly,
    Edit,
    Full,
}

impl NodePermissions {
    /// The YAML spelling, for diagnostics and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            NodePermissions::ReadOnly => "read-only",
            NodePermissions::Edit => "edit",
            NodePermissions::Full => "full",
        }
    }
}

/// `skip_serializing_if` for a flag whose absence means `false`.
fn is_false(flag: &bool) -> bool {
    !*flag
}

/// A node's crash-recovery policy — the full triple of options.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OnInterrupt {
    #[default]
    RestartNode,
    FailIfUncertain,
    /// Continue the same agent conversation — the
    /// `session_id` the log recorded (`agent_session_opened`) is
    /// handed back to the adapter's `resume`. Only `kind: prompt` opens
    /// a node-scoped session, so `check` refuses the explicit
    /// declaration anywhere else; an adapter without the
    /// `resume_session` capability — or a crash before any session
    /// opened — degrades to `restart_node` with an explicit
    /// `capability_degraded` event, never silently. As a *config default*
    /// (`defaults.on_interrupt`) it applies where a session exists;
    /// kinds without one (bash/check/…, and a loop's per-task sessions)
    /// restart, which is the only meaning the policy can have there.
    ResumeSession,
}

impl OnInterrupt {
    /// The YAML spelling, for diagnostics and the log.
    pub fn as_str(self) -> &'static str {
        match self {
            OnInterrupt::RestartNode => "restart_node",
            OnInterrupt::FailIfUncertain => "fail_if_uncertain",
            OnInterrupt::ResumeSession => "resume_session",
        }
    }
}

/// The node kinds built so far. `gate` and
/// `workflow` are the rest of the full catalogue and stay out until
/// their own turn.
/// What a `kind: loop` runs until. One condition exists: the ledger
/// has no task left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopUntil {
    AllTasksComplete,
}

impl LoopUntil {
    /// The YAML spelling, for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            LoopUntil::AllTasksComplete => "all_tasks_complete",
        }
    }
}

impl std::fmt::Display for LoopUntil {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Prompt {
        prompt: PromptSource,
    },
    Bash {
        run: String,
    },
    Loop {
        until: LoopUntil,
        prompt: PromptSource,
        /// Simultaneous `ready` tasks per batch — absent
        /// means the engine's own default, `1` (sequential; the batch
        /// mechanism handles both the same way, with no special case).
        /// Declared per-node, deliberately: no config-level default
        /// exists anywhere in the reference schema, since token spend
        /// multiplies with it and nobody should discover that from the
        /// bill instead of the workflow file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concurrency: Option<u32>,
        /// The agent never widens its own scope — it requests,
        /// the engine (or a person, in `ask`) decides. Absent has the
        /// same effect as declaring it with no `mode:` — `deny` (the
        /// default): every request becomes a finding, none are
        /// granted. Loop-scoped, not workflow- or config-scoped, because
        /// `scope_expansion_requested`'s own payload is keyed by
        /// `task_id` — this is ledger-task machinery, the same rung
        /// `concurrency:` already occupies on this node kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope_expansion: Option<ScopeExpansion>,
    },
    /// Nodes named by the author, run at once — distinct
    /// from a loop's own `concurrency:`, whose task count doesn't
    /// exist until the plan runs. Children are ordinary `Node`s (their
    /// own hooks/scope/artifacts/runner apply exactly as at the top
    /// level, dispatched through the same `execute_node`); they
    /// share the run's one worktree (one per *run*, not per
    /// node) and aren't visible to the top-level DAG's own `depends_on`.
    Parallel {
        #[serde(default)]
        join: JoinPolicy,
        /// `coordination:` — whether this group's
        /// children share a blackboard. `independent` (default): the
        /// blackboard tools are never even mounted — the right shape
        /// for evaluative groups (reviewers), where cross-contamination
        /// anchors judgments and kills the diversity the fan-out buys.
        /// `blackboard`: children with `run_tools` get
        /// `yunta_post_finding`/`yunta_get_blackboard` scoped to this
        /// group (never the whole run); reading siblings' posts still
        /// waits for the `join`.
        #[serde(default)]
        coordination: Coordination,
        nodes: Vec<Node>,
    },
    /// Automatic verification against data the engine already has —
    /// never a person (that's `gate`).
    /// The builtin list is closed on purpose: a `check` builtin is by
    /// definition something the engine can already evaluate; anything
    /// else is a `bash` node (exit code) or an `executor`.
    Check(CheckBuiltin),
    /// The extension point when neither `bash` (exit code only, no
    /// structured input) nor `check`'s closed builtin list covers it —
    /// external code, a JSON contract over stdio. `executor` names an
    /// entry in `skills.executors:`; `with` is opaque, executor-defined
    /// input. The contract fixes the high-level shape (JSON in, JSON out,
    /// exit code is the verdict) but stops short of naming fields,
    /// pending further design.
    Executor {
        executor: ExecutorName,
        #[serde(default)]
        with: serde_json::Map<String, serde_json::Value>,
        /// Absent means unenforced, same convention as `HookStep`'s own
        /// `timeout_seconds` — the design only calls for an
        /// engine-enforced timeout, without fixing a default or a field
        /// name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u64>,
    },
    /// A human decision resolved outside this process — v1's
    /// only shape is `external: {kind: pull_request, ...}`: the engine
    /// delegates the multi-person substrate (identity, permissions,
    /// notifications) to the team's forge instead of building a serve
    /// mode early. With `external: None` this is an **internal gate**
    /// — the shape the reference workflows' own `approve-plan`/
    /// `ship` use and the mode-coherence rule requiring every mode to
    /// include a gate option: resolved through `HumanInteraction`
    /// (console today, a richer resolution surface later) with the
    /// declared `options`, and `on:` mapping an option to an
    /// `on_failure.goto`-style re-route.
    Gate {
        /// Who the escalation names — a human on the forge (external),
        /// or whoever holds the interactive surface (internal).
        assignee: String,
        /// The question the internal gate asks — becomes the
        /// escalation's `summary`. Absent, a default derived from the node
        /// id is used.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// Declared choices, free ids (reference: `[aprobar, ajustar,
        /// abortar]`). Empty means the single default option `approve`;
        /// the engine always appends its own `abort` (aborting is
        /// always a valid exit, the same convention every escalation uses).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
        /// Option → re-route target (`on: { ajustar: plan }`): choosing
        /// a mapped option re-routes exactly like `on_failure.goto`
        /// — the target and its subgraph complete, then the
        /// gate returns to ready and asks again. Unbounded on purpose:
        /// each lap is human-driven, not an automatic cycle
        /// `max_reroutes` exists to cap. An unmapped option resolves
        /// the gate and the DAG continues.
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        on: IndexMap<String, NodeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        external: Option<ExternalGate>,
    },
    /// `kind: workflow`: runs another workflow as a
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
        /// `mounts:` — artifacts of the parent's own graph
        /// copied into the child's `run.dir/artifacts/` at birth: the
        /// promotion inheritance mechanism generalized (promotion is
        /// just one particular case of this general mechanism). The parent
        /// declares because the parent is who knows its own topology — a
        /// catalog child naming a sibling would be welded to one
        /// parent's shape and lose its standalone run. Each mount
        /// implies `depends_on` on the referenced node, which is what
        /// guarantees every sibling the child depends on has already
        /// finished before the child starts.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mounts: Vec<MountSpec>,
    },
}

impl NodeKind {
    /// Every `kind:` a node can declare, as written in YAML.
    pub const KINDS: &'static [&'static str] = &[
        "prompt", "bash", "loop", "parallel", "check", "executor", "gate", "workflow",
    ];

    /// The keys a node of `kind` accepts besides the node-level ones,
    /// or `None` for a kind that does not exist. The lists mirror the
    /// variants above; a test serializes each kind with every field set
    /// and checks nothing falls outside its list.
    pub fn keys(kind: &str) -> Option<&'static [&'static str]> {
        Some(match kind {
            "prompt" => &["prompt"],
            "bash" => &["run"],
            "loop" => &["until", "prompt", "concurrency", "scope_expansion"],
            "parallel" => &["join", "coordination", "nodes"],
            "check" => &["builtin", "max_severity"],
            "executor" => &["executor", "with", "timeout_seconds"],
            "gate" => &["assignee", "message", "options", "on", "external"],
            "workflow" => &["use", "inputs", "isolation", "mounts"],
            _ => return None,
        })
    }
}

/// One `mounts:` entry — `artifact:` is the only mount source there is,
/// kept as a named field (not a bare inline struct) so a second source
/// kind lands as a sibling field with the same untagged-by-field-name
/// convention `context:`/`on_finish:` already use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MountSpec {
    pub artifact: MountArtifact,
}

/// The mounted artifact: `node` names a node of the *parent's own*
/// graph — a `kind: workflow` sibling resolves through the recorded
/// link (`child_run_finished`) to that child run's artifacts, any other
/// node to the parent's own `run.dir/artifacts/`. `as:` renames the
/// copy in the child (absent keeps `name`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MountArtifact {
    pub node: NodeId,
    pub name: String,
    #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
    pub rename: Option<String>,
}

fn is_default_workflow_isolation(isolation: &WorkflowIsolation) -> bool {
    *isolation == WorkflowIsolation::default()
}

/// A `kind: workflow` node's `isolation:` — deliberately its own
/// enum, not [`crate::Isolation`]: `inherit` only exists for workflow
/// nodes, and a run-level `none` is not a per-node choice.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowIsolation {
    #[default]
    Worktree,
    Inherit,
}

/// `kind: gate`'s `external:` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalGate {
    pub kind: ForgeKind,
    /// Paths (relative to `run.dir`) committed to `branch` for review —
    /// the reference example: `[spec.md]`.
    pub artifacts: Vec<String>,
    /// Template-rendered branch name the artifacts are pushed to and the
    /// PR is opened from (`{{run.branch}}`, the reference example, resolves
    /// to `yunta/<run_id>` — a fresh push target, not necessarily the
    /// worktree's own local checkout branch, since `isolation: none`
    /// never creates one).
    pub branch: String,
}

/// The only forge integration v1 has — kept as a closed enum (not a
/// bare string) so a second one lands as a new variant with exhaustive
/// match-checking everywhere it matters, the same reasoning
/// `CheckBuiltin`'s own closed list uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ForgeKind {
    PullRequest,
}

/// `kind: check`'s closed builtin list. No budget builtin —
/// `limits:` already pauses the run on its own; duplicating that
/// as a check would be redundant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "builtin", rename_all = "snake_case", deny_unknown_fields)]
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

/// `parallel.coordination` — see the field's own doc on
/// [`NodeKind::Parallel`]. A closed enum, not a bool: a third
/// coordination shape (if one ever earns an ADR) lands as a variant
/// with exhaustive match-checking, same reasoning as `JoinPolicy`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Coordination {
    #[default]
    Independent,
    Blackboard,
}

/// `parallel.join`. `all` (default): the group finishes only
/// once every child does, and one failed child fails the group. `any`:
/// the group finishes with the first child to *succeed*; the engine
/// sends the rest `interrupt`, escalating to `kill` if they don't close
/// in time (same ordered-then-forceful mechanism as an agent session's
/// own cancellation).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JoinPolicy {
    #[default]
    All,
    Any,
}

/// A node's prompt: an inline string, or `{file: ...}`. The distinction
/// is **structural, never heuristic**: a scalar is
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
        use serde::de::Error;

        #[derive(Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        struct File {
            file: std::path::PathBuf,
        }

        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(PromptSource::Inline(text)),
            mapping @ Value::Mapping(_) => {
                let File { file } = nested::<D, _>("prompt", mapping)?;
                Ok(PromptSource::File(file))
            }
            other => Err(D::Error::custom(format!(
                "a prompt is text or `{{ file: <path> }}`, not {}",
                describe(&other)
            ))),
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

impl schemars::JsonSchema for PromptSource {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PromptSource".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "The prompt's text, or `{ file: <path> }` to read it from disk",
            "oneOf": [
                { "type": "string" },
                {
                    "type": "object",
                    "properties": { "file": { "type": "string" } },
                    "required": ["file"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

/// `artifacts.produces`. `task-ledger`, `findings` and
/// `questions` are interpreted. A plain string stays
/// opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Artifacts {
    pub produces: Vec<ArtifactSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ArtifactSpec {
    Plain(String),
    Typed { name: String, kind: ArtifactKind },
}

impl<'de> Deserialize<'de> for ArtifactSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        #[derive(Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        struct Typed {
            name: String,
            kind: ArtifactKind,
        }

        match Value::deserialize(deserializer)? {
            Value::String(name) => Ok(ArtifactSpec::Plain(name)),
            mapping @ Value::Mapping(_) => {
                let Typed { name, kind } = nested::<D, _>("artifact", mapping)?;
                Ok(ArtifactSpec::Typed { name, kind })
            }
            other => Err(D::Error::custom(format!(
                "an artifact is a name or `{{ name: <name>, kind: <kind> }}`, not {}",
                describe(&other)
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    TaskLedger,
    Findings,
    Questions,
}

/// `hooks: {before, after}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<HookStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<HookStep>,
}

/// One hook command. `timeout_seconds` is unenforced (no timeout) when
/// absent — additive over the earlier engine, which never had one.
/// Field name/units aren't pinned by the spec's prose ("timeout corto
/// configurable"); seconds fit hook-scale commands better than the
/// minutes granularity `defaults.timeout_minutes` uses for whole agent
/// sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    #[default]
    Fail,
    Warn,
}

/// `on_failure: {goto, max_reroutes}` — node-level re-routing.
/// `max_reroutes` is mandatory: a re-route without an explicit cap
/// is how a correction cycle turns infinite, so the schema refuses it.
/// Distinct from a hook's own `on_failure: fail|warn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OnFailure {
    pub goto: NodeId,
    pub max_reroutes: u32,
}

/// Reads a mapping that holds exactly one entry whose key is one of
/// `keys` — the shape of every value this schema discriminates by a
/// field name. `what` names the value in the error.
fn keyed_entry<'de, D: Deserializer<'de>>(
    deserializer: D,
    what: &str,
    keys: &[&str],
) -> Result<(String, Value), D::Error> {
    use serde::de::Error;

    let mut entries = Mapping::deserialize(deserializer)?.into_iter();
    let (key, value) = match (entries.next(), entries.next()) {
        (Some(entry), None) => entry,
        _ => {
            return Err(D::Error::custom(format!(
                "{what} is a mapping with exactly one key, one of {}",
                list(keys)
            )))
        }
    };
    let Some(key) = key.as_str() else {
        return Err(D::Error::custom(format!(
            "{what} is keyed by a string, one of {}",
            list(keys)
        )));
    };
    if !keys.contains(&key) {
        return Err(D::Error::custom(format!(
            "unknown key `{key}` for {what}; one of {}",
            list(keys)
        )));
    }
    Ok((key.to_string(), value))
}

/// Parses the value found under `key`, keeping `key` in the error's
/// path so the location stays complete once the parser adds its own.
fn nested<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    key: &str,
    value: Value,
) -> Result<T, D::Error> {
    use serde::de::Error;

    yaml::from_value(value).map_err(|error| {
        D::Error::custom(match error {
            YamlError::Parse { path, message } if path.is_empty() || path == "." => {
                format!("{key}: {message}")
            }
            YamlError::Parse { path, message } => format!("{key}.{path}: {message}"),
            other => other.to_string(),
        })
    })
}

/// `` `a`, `b`, `c` `` — how every error here lists keys.
fn list<S: AsRef<str>>(keys: &[S]) -> String {
    keys.iter()
        .map(|key| format!("`{}`", key.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What kind of YAML value `value` is, for an error that expected another.
fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a list",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iter_nodes_with_group_pairs_each_parallel_child_with_its_group() {
        let workflow: Workflow = yaml::parse(
            r#"
name: groups
nodes:
  - { id: solo, kind: bash, run: x }
  - id: fan
    kind: parallel
    nodes:
      - { id: left, kind: bash, run: x }
      - { id: right, kind: bash, run: x }
"#,
        )
        .unwrap();
        let pairs: Vec<(&str, Option<&str>)> = workflow
            .iter_nodes_with_group()
            .map(|(node, group)| (node.id.as_str(), group.map(|g| g.id.as_str())))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("solo", None),
                ("fan", None),
                ("left", Some("fan")),
                ("right", Some("fan")),
            ],
        );
    }

    /// One node per kind with every key its kind accepts, so that
    /// serializing it shows exactly which keys the variant owns.
    const EVERY_KIND: &str = r#"
name: kinds
nodes:
  - { id: p, kind: prompt, prompt: x }
  - { id: b, kind: bash, run: x }
  - id: l
    kind: loop
    until: all_tasks_complete
    prompt: x
    concurrency: 2
    scope_expansion: { mode: ask, within: ["src/**"], max_per_run: 1 }
  - id: g
    kind: parallel
    join: any
    coordination: blackboard
    nodes: [{ id: c, kind: bash, run: x }]
  - { id: k, kind: check, builtin: findings_gate, max_severity: major }
  - { id: e, kind: executor, executor: lint, with: { a: 1 }, timeout_seconds: 5 }
  - id: t
    kind: gate
    assignee: lead
    message: m
    options: [approve]
    on: { approve: p }
    external: { kind: pull_request, artifacts: [a], branch: b }
  - id: w
    kind: workflow
    use: child
    inputs: { a: b }
    isolation: inherit
    mounts: [{ artifact: { node: p, name: n, as: m } }]
"#;

    #[test]
    fn every_kind_lists_exactly_the_keys_its_variant_serializes() {
        let workflow: Workflow = yaml::parse(EVERY_KIND).unwrap();
        assert_eq!(workflow.nodes.len(), NodeKind::KINDS.len());
        for node in &workflow.nodes {
            let text = yaml::to_string(node).unwrap();
            let mapping: Mapping = yaml::parse(&text).unwrap();
            let kind = mapping["kind"].as_str().unwrap().to_string();
            let listed =
                NodeKind::keys(&kind).unwrap_or_else(|| panic!("`{kind}` has no key list"));
            let mut serialized: Vec<String> = mapping
                .keys()
                .filter_map(Value::as_str)
                .filter(|key| *key != "kind" && !NODE_KEYS.contains(key))
                .map(str::to_string)
                .collect();
            serialized.sort();
            let mut expected: Vec<String> = listed.iter().map(|key| key.to_string()).collect();
            expected.sort();
            assert_eq!(serialized, expected, "kind `{kind}` serializes keys its list does not name, or lists keys it never writes");
        }
    }

    #[test]
    fn the_node_level_keys_are_exactly_what_the_node_fields_accept() {
        // Every node-level key must be one `NodeFields` reads, or a node
        // written with it would be refused; every field must be listed,
        // or it would be handed to the kind and refused there.
        let full: Node = yaml::parse(
            r#"
id: n
kind: prompt
prompt: x
depends_on: [a]
scope: ["src/**"]
runners: [r1, r2]
agent: a
artifacts: { produces: [x] }
hooks: { before: [{ run: x }] }
on_failure: { goto: a, max_reroutes: 1 }
on_interrupt: restart_node
description: d
permissions: edit
network: true
context: [{ command: x }]
skills: [s]
interactive: true
invariant: true
"#,
        )
        .unwrap();
        let text = yaml::to_string(&full).unwrap();
        let mapping: Mapping = yaml::parse(&text).unwrap();
        let mut serialized: Vec<&str> = mapping
            .keys()
            .filter_map(Value::as_str)
            .filter(|key| *key != "kind" && *key != "prompt")
            .collect();
        serialized.sort_unstable();
        let mut listed: Vec<&str> = NODE_KEYS
            .iter()
            .copied()
            .filter(|key| *key != "runner")
            .collect();
        listed.sort_unstable();
        assert_eq!(serialized, listed);
    }
}

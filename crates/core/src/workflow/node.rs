//! A node as authored: the fields every kind shares, the keys a node
//! accepts at its own level, and the policies that qualify it
//! (`permissions`, `on_interrupt`, `until`).

use serde::{Deserialize, Deserializer, Serialize};

use super::parse::{describe, list};
use super::{Artifacts, ContextSpec, Hooks, NodeKind, OnFailure};
use crate::ids::{AgentName, NodeId, RunnerName};
use crate::yaml::{self, Mapping, Value};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Workflow;
    use crate::yaml::{self, Mapping, Value};

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

//! `kind:` — the node kinds and the shape each one carries.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::parse::{describe, nested};
use super::{LoopUntil, Node, ScopeExpansion};
use crate::ids::{ExecutorName, NodeId, OptionId};
use crate::yaml::Value;

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
    /// exit code is the verdict); the field names are registered debt
    /// A-12 in `docs/design/deuda-consciente.md`.
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
        options: Vec<OptionId>,
        /// Option → re-route target (`on: { ajustar: plan }`): choosing
        /// a mapped option re-routes exactly like `on_failure.goto`
        /// — the target and its subgraph complete, then the
        /// gate returns to ready and asks again. Unbounded on purpose:
        /// each lap is human-driven, not an automatic cycle
        /// `max_reroutes` exists to cap. An unmapped option resolves
        /// the gate and the DAG continues.
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        on: IndexMap<OptionId, NodeId>,
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

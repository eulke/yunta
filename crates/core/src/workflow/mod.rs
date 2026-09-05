//! The workflow schema: what `.yunta/workflows/<name>.yaml` declares,
//! parsed once at the frontier.
//!
//! A person writes these files, so every type here refuses a key it
//! does not know and names the keys it does. The values the schema
//! discriminates by a field name instead of a tag — a `context:` entry,
//! an `on_finish:` step, an artifact, a prompt, a mode's `include` —
//! read that name explicitly and say which names exist when it is
//! missing or wrong, instead of reporting that nothing matched.

mod artifacts;
mod context;
mod hooks;
mod node;
mod node_kind;
mod parse;

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::{ModeName, NodeId};
use crate::inputs::InputSpec;
use crate::yaml::Value;
use parse::{describe, keyed_entry, nested};

pub use artifacts::{ArtifactKind, ArtifactSpec, Artifacts};
pub use context::{
    ArtifactContextRef, ContextSpec, KnowledgeLayer, KnowledgeParams, LedgerParams, McpQueryParams,
    NodeOutputParams, RunEventsFilter, RunEventsParams, ScopeExpansion,
};
pub use hooks::{HookFailurePolicy, HookStep, Hooks, OnFailure};
pub use node::{LoopUntil, Node, NodeDefaults, NodePermissions, OnInterrupt};
pub use node_kind::{
    CheckBuiltin, Coordination, ExternalGate, ForgeKind, JoinPolicy, MountArtifact, MountSpec,
    NodeKind, PromptSource, WorkflowIsolation,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml;

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
}

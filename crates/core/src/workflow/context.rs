//! `context:` entries — the sources a node reads before it runs, each
//! discriminated by the key it is written under.

use serde::{Deserialize, Deserializer, Serialize};

use super::parse::{keyed_entry, nested};
use crate::ids::NodeId;

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

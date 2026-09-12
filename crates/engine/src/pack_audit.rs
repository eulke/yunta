//! `yunta pack audit`: a full static inventory
//! of everything a pack's own workflows would do — every `bash`/hook/
//! loop-`until` command, every context source and what it points at,
//! per-node permissions, required `agent:`, `mcp` servers a node's
//! context reaches, executors marked as code, and each workflow's
//! **full, untrimmed** prompt text (inline or read from its referenced
//! file). Inventory, never verdict: there is no pattern-matching
//! for "suspicious" natural-language content — that would be trivially
//! evadible and would give false confidence. Nothing here runs
//! anything; whether the pack's own tests pass is a separate,
//! IO-heavy step the CLI layer owns (it needs an adapter and a sandbox,
//! neither of which this module touches).

use std::path::{Path, PathBuf};

use yunta_core::{
    AgentName, ContextSpec, ExecutorName, Hooks, Node, NodeKind, PackManifest, PromptSource,
    Workflow,
};

/// The full inventory of one installed (or freshly cloned, pre-vendor)
/// pack directory.
#[derive(Debug)]
pub struct PackAudit {
    pub manifest: PackManifest,
    pub workflows: Vec<WorkflowAudit>,
}

/// One workflow declared under `contents.workflows` — kept even when it
/// fails to read or parse, since a pack whose manifest promises a
/// workflow that isn't actually there (or doesn't parse) is itself
/// something the inventory must surface, not silently drop.
#[derive(Debug)]
pub struct WorkflowAudit {
    /// The `contents.workflows` entry this came from, relative to the
    /// pack root.
    pub declared_path: String,
    pub error: Option<String>,
    pub nodes: Vec<NodeAudit>,
}

/// A `{file: ...}` prompt naming something the pack does not ship.
#[derive(Debug, thiserror::Error)]
#[error("cannot read `{path}`")]
pub struct PromptReadError {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

/// A prompt's full text, resolved exactly as the engine would resolve
/// it at manifest-freeze time — `Ok` for both inline text and a
/// successfully read `{file: ...}`; `Err` when a `{file: ...}` prompt
/// names something the pack doesn't actually ship.
pub type PromptText = Result<String, PromptReadError>;

#[derive(Debug)]
pub struct NodeAudit {
    pub id: String,
    pub kind: &'static str,
    /// `bash`'s own `run:`, or a `loop`'s `until:` — the one command
    /// that kind carries, if any.
    pub command: Option<String>,
    pub hooks_before: Vec<String>,
    pub hooks_after: Vec<String>,
    /// `prompt`/`loop` nodes only — `None` for every other kind.
    pub prompt: Option<PromptText>,
    /// One human-readable line per `context:` entry, naming the source
    /// and exactly what it points at.
    pub context: Vec<String>,
    pub permissions: Option<&'static str>,
    pub agent: Option<AgentName>,
    /// MCP server names reached by this node's own `context: - mcp:`
    /// entries.
    pub mcp_servers: Vec<String>,
    /// `kind: executor`'s own name — code, not declarative content;
    /// flagged separately from everything else in the inventory.
    pub executor: Option<ExecutorName>,
}

/// Audits every workflow the manifest declares under `contents.workflows`,
/// reading each from `pack_dir` (the pack's own root — either its vendored
/// location or a freshly cloned, not-yet-vendored checkout; both lay out
/// `contents.workflows` paths the same way, relative to that root).
pub fn audit_pack(pack_dir: &Path, manifest: PackManifest) -> PackAudit {
    let workflows = manifest
        .contents
        .workflows
        .iter()
        .map(|declared| audit_workflow(pack_dir, declared))
        .collect();
    PackAudit {
        manifest,
        workflows,
    }
}

fn audit_workflow(pack_dir: &Path, declared: &str) -> WorkflowAudit {
    let path = pack_dir.join(declared);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            return WorkflowAudit {
                declared_path: declared.to_string(),
                error: Some(format!("cannot read `{}`: {e}", path.display())),
                nodes: Vec::new(),
            }
        }
    };
    let workflow: Workflow = match yunta_core::yaml::parse(&text) {
        Ok(workflow) => workflow,
        Err(e) => {
            return WorkflowAudit {
                declared_path: declared.to_string(),
                error: Some(format!("`{}` fails to parse: {e}", path.display())),
                nodes: Vec::new(),
            }
        }
    };
    let workflow_dir = path.parent().unwrap_or(pack_dir);
    let node_defaults_hooks = workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref());
    let nodes = workflow
        .iter_nodes()
        .map(|node| audit_node(node, workflow_dir, node_defaults_hooks))
        .collect();
    WorkflowAudit {
        declared_path: declared.to_string(),
        error: None,
        nodes,
    }
}

/// A node's hooks with `node_defaults.hooks` filled in per phase — the
/// same "own list replaces, empty inherits" rule the run-time
/// `effective_hooks` applies, reimplemented here in pure form:
/// the run-time version takes a live `RunCtx`, which an audit — reading
/// a pack directory that may not even be installed yet — has no reason
/// to construct.
fn effective_hooks<'a>(
    node: &'a Node,
    node_defaults_hooks: Option<&'a Hooks>,
) -> (&'a [yunta_core::HookStep], &'a [yunta_core::HookStep]) {
    let own = node.hooks.as_ref();
    let before = own
        .filter(|hooks| !hooks.before.is_empty())
        .map(|hooks| hooks.before.as_slice())
        .or_else(|| node_defaults_hooks.map(|hooks| hooks.before.as_slice()))
        .unwrap_or(&[]);
    let after = own
        .filter(|hooks| !hooks.after.is_empty())
        .map(|hooks| hooks.after.as_slice())
        .or_else(|| node_defaults_hooks.map(|hooks| hooks.after.as_slice()))
        .unwrap_or(&[]);
    (before, after)
}

fn audit_node(node: &Node, workflow_dir: &Path, node_defaults_hooks: Option<&Hooks>) -> NodeAudit {
    let (before, after) = effective_hooks(node, node_defaults_hooks);

    let (kind, command, prompt, executor) = match &node.kind {
        NodeKind::Prompt { prompt } => (
            "prompt",
            None,
            Some(resolve_prompt(prompt, workflow_dir)),
            None,
        ),
        NodeKind::Bash { run } => ("bash", Some(run.clone()), None, None),
        NodeKind::Loop { until, prompt, .. } => (
            "loop",
            Some(until.as_str().to_string()),
            Some(resolve_prompt(prompt, workflow_dir)),
            None,
        ),
        NodeKind::Parallel { .. } => ("parallel", None, None, None),
        NodeKind::Check(_) => ("check", None, None, None),
        NodeKind::Executor { executor, .. } => ("executor", None, None, Some(executor.clone())),
        NodeKind::Gate { .. } => ("gate", None, None, None),
        NodeKind::Workflow { .. } => ("workflow", None, None, None),
    };

    NodeAudit {
        id: node.id.as_str().to_string(),
        kind,
        command,
        hooks_before: before.iter().map(|step| step.run.clone()).collect(),
        hooks_after: after.iter().map(|step| step.run.clone()).collect(),
        prompt,
        context: node.context.iter().map(describe_context).collect(),
        permissions: node.permissions.map(yunta_core::NodePermissions::as_str),
        agent: node.agent.clone(),
        mcp_servers: node
            .context
            .iter()
            .filter_map(|spec| match spec {
                ContextSpec::Mcp { mcp } => Some(mcp.server.clone()),
                _ => None,
            })
            .collect(),
        executor,
    }
}

fn resolve_prompt(prompt: &PromptSource, workflow_dir: &Path) -> PromptText {
    match prompt {
        PromptSource::Inline(text) => Ok(text.clone()),
        PromptSource::File(path) => {
            let full_path = workflow_dir.join(path);
            std::fs::read_to_string(&full_path).map_err(|source| PromptReadError {
                path: full_path.clone(),
                source,
            })
        }
    }
}

fn describe_context(spec: &ContextSpec) -> String {
    match spec {
        ContextSpec::Files { files } => format!("files: {}", files.join(", ")),
        ContextSpec::Command { command } => format!("command: {command}"),
        ContextSpec::Artifact { artifact } => match &artifact.node {
            Some(node) => format!("artifact: node={node} name={}", artifact.name),
            None => format!("artifact: name={} (mounted, no producer)", artifact.name),
        },
        ContextSpec::Mcp { mcp } => format!("mcp: server={} query={}", mcp.server, mcp.query),
        ContextSpec::RunEvents { run_events } => match &run_events.filter {
            Some(filter) => format!("run-events: filter={}", filter.as_str()),
            None => "run-events: (no filter)".to_string(),
        },
        ContextSpec::Tasks { .. } => "tasks".to_string(),
        ContextSpec::Knowledge { knowledge } => {
            if knowledge.layers.is_empty() {
                "knowledge: (every layer)".to_string()
            } else {
                format!(
                    "knowledge: layers=[{}]",
                    knowledge
                        .layers
                        .iter()
                        .map(|l| l.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        ContextSpec::NodeOutput { node_output } => {
            format!("node-output: node={}", node_output.node)
        }
    }
}

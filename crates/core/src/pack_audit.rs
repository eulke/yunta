//! What a pack's audit found: a full static inventory of everything its own
//! workflows would do, each workflow kept even when it does not read, and
//! each node with every command, context source, permission, agent, server,
//! executor and repository path it names, and its full prompt. The engine
//! builds it from a pack's files, and a page says it; nothing here reads.

use std::path::PathBuf;

use crate::{AgentName, ExecutorName, PackManifest};

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
    pub error: Option<WorkflowUnread>,
    pub nodes: Vec<NodeAudit>,
}

/// Why a pack's workflow has no nodes to show.
#[derive(Debug)]
pub enum WorkflowUnread {
    /// The file could not be read.
    Unreadable(UnreadFile),
    /// The language refuses what the file says: every problem, and the text they are in.
    Refused {
        report: Box<crate::diagnostic::Report>,
        text: String,
    },
}

/// A file of the pack the audit could not read: a workflow it declares, or
/// a `{file: ...}` prompt naming something it does not ship.
#[derive(Debug, thiserror::Error)]
#[error("cannot read `{path}`")]
pub struct UnreadFile {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

/// A prompt's full text, resolved exactly as the engine would resolve
/// it at manifest-freeze time — `Ok` for both inline text and a
/// successfully read `{file: ...}`; `Err` when a `{file: ...}` prompt
/// names something the pack doesn't actually ship.
pub type PromptText = Result<String, UnreadFile>;

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
    pub mcp_servers: Vec<crate::McpServerName>,
    /// `kind: executor`'s own name — code, not declarative content;
    /// flagged separately from everything else in the inventory.
    pub executor: Option<ExecutorName>,
    /// The repository paths the node names: the globs its `scope:`
    /// declares — what makes a workflow assume the layout of a repository
    /// it may never see. `scope: run` names none.
    pub paths: Vec<String>,
}

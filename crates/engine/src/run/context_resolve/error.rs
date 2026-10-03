//! What resolving a node's `context:` can fail with: one variant per
//! source and cause, with the failing source named, rendered once at
//! the edge.

use std::path::PathBuf;

use thiserror::Error;
use yunta_core::McpServerName;
use yunta_core::{CommitSha, NodeId};

use yunta_core::template::TemplateError;

use super::mcp::McpQueryError;
use super::EXTERNAL_CALL_TIMEOUT;

#[derive(Debug, Error)]
pub(super) enum ContextResolveError {
    #[error("context `{source_id}` on node `{node}`: failed to {action}: {source}")]
    Io {
        node: NodeId,
        source_id: String,
        action: String,
        #[source]
        source: std::io::Error,
    },
    /// A `files:` entry naming a path the node's tree does not hold —
    /// said in the terms of where the file has to be for the next attempt
    /// to find it, not of the checkout this attempt happened to read.
    #[error("context `{source_id}` on node `{node}`: {absence}")]
    MissingFile {
        node: NodeId,
        source_id: String,
        absence: Absence,
    },
    #[error("context `{source_id}` on node `{node}`: {source}")]
    Process {
        node: NodeId,
        source_id: String,
        #[source]
        source: crate::process::SpawnError,
    },
    #[error("context `{source_id}` on node `{node}`: {source}")]
    Template {
        node: NodeId,
        source_id: String,
        #[source]
        source: TemplateError,
    },
    #[error("context `{source_id}` on node `{node}`: command `{cmd}` exited {status}: {stderr}")]
    CommandFailed {
        node: NodeId,
        source_id: String,
        cmd: String,
        status: i32,
        stderr: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: command `{cmd}` did not finish within {}s",
        EXTERNAL_CALL_TIMEOUT.as_secs()
    )]
    CommandTimedOut {
        node: NodeId,
        source_id: String,
        cmd: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: command `{cmd}` was stopped by a cancellation"
    )]
    Cancelled {
        node: NodeId,
        source_id: String,
        cmd: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: the {}{} was never produced — \
         this run's log holds no such artifact",
        .artifact.label(),
        .referenced.as_ref().map(|r| format!(" (declared by node `{r}`)")).unwrap_or_default()
    )]
    MissingArtifact {
        node: NodeId,
        source_id: String,
        /// `None` for the node-less form: the question is about the run
        /// rather than about one node, producer unnamed on purpose.
        referenced: Option<NodeId>,
        /// What the source asked for: the identity, which is what a log
        /// answers by.
        artifact: yunta_core::events::ArtifactId,
    },
    #[error(
        "context `{source_id}` on node `{node}`: node `{referenced}` has no captured output \
         (only `kind: bash` nodes capture output)"
    )]
    MissingNodeOutput {
        node: NodeId,
        source_id: String,
        referenced: NodeId,
    },
    #[error("context `{source_id}` on node `{node}`: failed to render the run's events: {detail}")]
    RunEventsRender {
        node: NodeId,
        source_id: String,
        detail: String,
    },
    /// Between org knowledge packs there is no order —
    /// same filename from two installed packs never resolves by
    /// alphabetical or install order, it names both and stops.
    #[error(
        "context `{source_id}` on node `{node}`: knowledge file `{file}` is shipped by two \
         installed packs — `{pack_a}` and `{pack_b}` — and the org layer has no precedence \
         between packs; remove one, or shadow the file with the repo's own \
         `.yunta/knowledge/{file}`"
    )]
    OrgKnowledgeCollision {
        node: NodeId,
        source_id: String,
        file: String,
        pack_a: String,
        pack_b: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: mcp server `{server}` is not declared in \
         `mcp_servers:`"
    )]
    UnknownMcpServer {
        node: NodeId,
        source_id: String,
        server: McpServerName,
    },
    #[error(
        "context `{source_id}` on node `{node}`: mcp server `{server}` declares `auth_env: \
         {var}`, but that environment variable isn't set"
    )]
    MissingAuthEnv {
        node: NodeId,
        source_id: String,
        server: McpServerName,
        var: String,
    },
    #[error("context `{source_id}` on node `{node}`: mcp server `{server}`: {source}")]
    McpFailed {
        node: NodeId,
        source_id: String,
        server: McpServerName,
        #[source]
        source: McpQueryError,
    },
    #[error(
        "context `{source_id}` on node `{node}`: mcp server `{server}` did not respond within \
         {}s",
        EXTERNAL_CALL_TIMEOUT.as_secs()
    )]
    McpTimedOut {
        node: NodeId,
        source_id: String,
        server: McpServerName,
    },
}

/// Where a missing `files:` path was looked for, which is what says where
/// a person puts it.
#[derive(Debug)]
pub(super) enum Absence {
    /// An absolute path: the run's tree has nothing to do with it.
    Absolute { path: String },
    /// A path inside the run's tree. `branched_from` is the commit an
    /// isolated run's tree starts from — named because a file the person
    /// has only in their own checkout is the likeliest way to get here —
    /// and `None` for a run working in that checkout itself.
    InRunTree {
        path: String,
        run_tree: PathBuf,
        branched_from: Option<CommitSha>,
    },
}

impl std::fmt::Display for Absence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Absence::Absolute { path } => write!(f, "`{path}` does not exist"),
            Absence::InRunTree {
                path,
                run_tree,
                branched_from: Some(base),
            } => write!(
                f,
                "`{path}` is not in the run's tree, which starts from commit `{}` — a file \
                 that is not committed there, or that git ignores, never reaches it; put it \
                 at `{}` and choose `retry`, or declare the entry `optional: true` if the node \
                 can do without it",
                base.abbreviated(),
                run_tree.join(path).display()
            ),
            Absence::InRunTree {
                path,
                run_tree,
                branched_from: None,
            } => write!(
                f,
                "`{path}` does not exist in `{}`; put it there and choose `retry`, or declare \
                 the entry `optional: true` if the node can do without it",
                run_tree.display()
            ),
        }
    }
}

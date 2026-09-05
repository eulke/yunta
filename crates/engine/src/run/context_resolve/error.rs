//! What resolving a node's `context:` can fail with: one variant per
//! source and cause, with the failing source named, rendered once at
//! the edge.

use thiserror::Error;
use yunta_core::NodeId;

use crate::template::TemplateError;

use super::mcp::McpQueryError;
use super::EXTERNAL_CALL_TIMEOUT;

#[derive(Debug, Error)]
pub(super) enum ContextResolveError {
    #[error("context `{source_id}` on node `{node}`: failed to {action}")]
    Io {
        node: NodeId,
        source_id: String,
        action: String,
        #[source]
        source: std::io::Error,
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
        "context `{source_id}` on node `{node}`: artifact `{name}`{} was never produced — \
         nothing wrote it into this run's `artifacts/`",
        .referenced.as_ref().map(|r| format!(" (declared by node `{r}`)")).unwrap_or_default()
    )]
    MissingArtifact {
        node: NodeId,
        source_id: String,
        /// `None` for the node-less form: the read is against
        /// this run's own `artifacts/`, producer unnamed on purpose.
        referenced: Option<NodeId>,
        name: String,
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
        server: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: mcp server `{server}` declares `auth_env: \
         {var}`, but that environment variable isn't set"
    )]
    MissingAuthEnv {
        node: NodeId,
        source_id: String,
        server: String,
        var: String,
    },
    #[error("context `{source_id}` on node `{node}`: mcp server `{server}`: {source}")]
    McpFailed {
        node: NodeId,
        source_id: String,
        server: String,
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
        server: String,
    },
}

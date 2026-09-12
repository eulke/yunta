//! The sources a `context:` entry resolves from: files, a command's
//! output, an artifact, the run's own events, the ledger, and a node's
//! captured output — plus writing that output where the next node reads it.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, StoredEvent};
use yunta_core::{sha256_hex, ContentHash, Node, NodeId};

use crate::process::{spawn_governed, GovernedCommand, Outcome};
use crate::template::render_template;

use super::error::ContextResolveError;
use super::EXTERNAL_CALL_TIMEOUT;
use crate::run::node_exec::template_vars;
use crate::run::{RunCtx, RunError};

pub(super) async fn resolve_files(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    files: &[String],
) -> Result<Vec<u8>, ContextResolveError> {
    let vars = template_vars(ctx, node);
    let mut out = Vec::new();
    for pattern in files {
        let rendered =
            render_template(pattern, &vars).map_err(|e| ContextResolveError::Template {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                source: e,
            })?;
        let path = if Path::new(&rendered).is_absolute() {
            PathBuf::from(&rendered)
        } else {
            ctx.worktree.join(&rendered)
        };
        let bytes = std::fs::read(&path).map_err(|source| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: format!("read `{}`", path.display()),
            source,
        })?;
        out.extend_from_slice(format!("# {rendered}\n").as_bytes());
        out.extend_from_slice(&bytes);
        out.push(b'\n');
    }
    Ok(out)
}

pub(super) async fn resolve_command(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    command: &str,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, ContextResolveError> {
    let vars = template_vars(ctx, node);
    let rendered = render_template(command, &vars).map_err(|e| ContextResolveError::Template {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        source: e,
    })?;
    let command = GovernedCommand::shell(ctx.worktree, &rendered).timeout(EXTERNAL_CALL_TIMEOUT);
    let (status, stdout, stderr) = match spawn_governed(command, ctx.supervision(cancel))
        .await
        .map_err(|source| ContextResolveError::Process {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            source,
        })? {
        Outcome::Exited {
            status,
            stdout,
            stderr,
        } => (status, stdout, stderr),
        Outcome::TimedOut { .. } => {
            return Err(ContextResolveError::CommandTimedOut {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                cmd: rendered,
            });
        }
        Outcome::Cancelled { .. } => {
            return Err(ContextResolveError::Cancelled {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                cmd: rendered,
            });
        }
    };
    if !status.success() {
        return Err(ContextResolveError::CommandFailed {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            cmd: rendered,
            status: status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
        });
    }
    Ok(stdout)
}

pub(super) async fn resolve_artifact(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    artifact: &yunta_core::ArtifactContextRef,
) -> Result<Vec<u8>, ContextResolveError> {
    let path = ctx.run_dir.join("artifacts").join(&artifact.name);
    std::fs::read(&path).map_err(|_| ContextResolveError::MissingArtifact {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        referenced: artifact.node.clone(),
        name: artifact.name.clone(),
    })
}

pub(super) async fn resolve_run_events(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::RunEventsParams,
) -> Result<Vec<u8>, ContextResolveError> {
    let events = ctx
        .load_events()
        .await
        .map_err(|e| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: "read the event log".to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
    // The same canonical JSONL the run's own `events.jsonl` export
    // writes — a session reads exactly what a forensic reader does,
    // stable across any `Debug` derive change on the event structs.
    let filtered: Vec<StoredEvent> = match params.filter {
        None => events,
        Some(yunta_core::RunEventsFilter::Failed) => events
            .into_iter()
            .filter(|e| matches!(e.payload(), Some(EventPayload::NodeFailed(_))))
            .collect(),
        // History, not state: a session that mounts events wants what
        // happened, and a finding that was rewritten or taken back is
        // part of that. A session that wants the set standing now mounts
        // the findings artifact.
        Some(yunta_core::RunEventsFilter::Findings) => events
            .into_iter()
            .filter(|e| {
                matches!(
                    e.payload(),
                    Some(
                        EventPayload::FindingPosted(_)
                            | EventPayload::FindingUpdated(_)
                            | EventPayload::FindingWithdrawn(_)
                    )
                )
            })
            .collect(),
    };
    let jsonl = crate::events_export::render_events_jsonl(&filtered).map_err(|source| {
        ContextResolveError::RunEventsRender {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            detail: source.to_string(),
        }
    })?;
    Ok(jsonl.into_bytes())
}

pub(super) async fn resolve_ledger(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
) -> Result<Vec<u8>, ContextResolveError> {
    // A storage failure is not an empty ledger: handing the agent "no
    // tasks" as context would hide the failure behind plausible content,
    // so the read propagates exactly as its sibling `resolve_run_events`
    // already does.
    let state = ctx
        .run_view()
        .await
        .map_err(|e| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: "read the event log".to_string(),
            source: std::io::Error::other(e.to_string()),
        })?
        .state;
    let mut lines: Vec<String> = state
        .tasks
        .iter()
        .map(|(id, status)| format!("{id}: {status:?}"))
        .collect();
    lines.sort();
    Ok(lines.join("\n").into_bytes())
}

pub(super) async fn resolve_node_output(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::NodeOutputParams,
) -> Result<Vec<u8>, ContextResolveError> {
    let path = node_output_path(ctx.run_dir, &params.node);
    std::fs::read(&path).map_err(|_| ContextResolveError::MissingNodeOutput {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        referenced: params.node.clone(),
    })
}

fn node_output_path(run_dir: &Path, node_id: &NodeId) -> PathBuf {
    run_dir
        .join("node-output")
        .join(format!("{}.txt", node_id.as_str()))
}

/// Captures a `kind: bash` node's own stdout/stderr right after it
/// exits — called regardless of exit status, since a *failing*
/// node's output is exactly what a corrective node's `node-output`
/// context wants to read.
pub(in crate::run) fn write_node_output(
    run_dir: &Path,
    node_id: &NodeId,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), RunError> {
    let path = node_output_path(run_dir, node_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RunError::Io {
            context: format!("create node-output directory for `{node_id}`"),
            source,
        })?;
    }
    let mut content = Vec::new();
    content.extend_from_slice(b"stdout:\n");
    content.extend_from_slice(stdout);
    content.extend_from_slice(b"\n\nstderr:\n");
    content.extend_from_slice(stderr);
    content.push(b'\n');
    std::fs::write(&path, content).map_err(|source| RunError::Io {
        context: format!("write captured output for node `{node_id}`"),
        source,
    })
}

pub(super) fn materialize(
    run_dir: &Path,
    content: &[u8],
) -> std::io::Result<(PathBuf, ContentHash)> {
    let hash = sha256_hex(content);
    let dir = run_dir.join("context").join(hash.as_str());
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("content");
    std::fs::write(&path, content)?;
    Ok((path, hash))
}

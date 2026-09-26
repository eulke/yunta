//! The sources a `context:` entry resolves from: files, a command's
//! output, an artifact, the run's own events, the tasks document, and a node's
//! captured output — plus writing that output where the next node reads it.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, StoredEvent};
use yunta_core::{ContentHash, Node, NodeId};

use crate::artifacts::store::ObjectStore;
use crate::process::{spawn_governed, GovernedCommand, Outcome};
use yunta_core::template::render_template;

use super::error::{Absence, ContextResolveError};
use super::Resolved;
use super::EXTERNAL_CALL_TIMEOUT;
use crate::run::node_exec::template_vars;
use crate::run::{RunCtx, RunError};
use yunta_core::events::{FindingEvent, NodeEvent};

/// What the session reads in place of an optional file that is not
/// there: said, never left for the agent to guess at (D186).
const ABSENT_MARKER: &str = "[absent — declared optional; not in the run's tree]";

pub(super) async fn resolve_files(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    files: &[yunta_core::ContextFile],
) -> Result<Resolved, ContextResolveError> {
    let vars = template_vars(ctx, node);
    let mut out = Vec::new();
    let mut absent = Vec::new();
    for file in files {
        let rendered =
            render_template(&file.path, &vars).map_err(|e| ContextResolveError::Template {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                source: e,
            })?;
        let path = if Path::new(&rendered).is_absolute() {
            PathBuf::from(&rendered)
        } else {
            ctx.worktree.join(&rendered)
        };
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound && file.optional => {
                out.extend_from_slice(format!("# {rendered}\n{ABSENT_MARKER}\n").as_bytes());
                absent.push(rendered);
                continue;
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(ContextResolveError::MissingFile {
                    node: node.id.clone(),
                    source_id: source_id.to_string(),
                    absence: absence(ctx, rendered),
                });
            }
            Err(source) => {
                return Err(ContextResolveError::Io {
                    node: node.id.clone(),
                    source_id: source_id.to_string(),
                    action: format!("read `{}`", path.display()),
                    source,
                });
            }
        };
        out.extend_from_slice(format!("# {rendered}\n").as_bytes());
        out.extend_from_slice(&bytes);
        out.push(b'\n');
    }
    Ok(Resolved { bytes: out, absent })
}

/// Where `rendered` was looked for, in the terms a person acts on: the
/// run's own tree — never the checkout a node given one of its own reads,
/// which is rebuilt from the run's tree on its next attempt — and, for an
/// isolated run, the commit that tree starts from.
fn absence(ctx: &RunCtx<'_>, rendered: String) -> Absence {
    if Path::new(&rendered).is_absolute() {
        return Absence::Absolute { path: rendered };
    }
    let run_tree = ctx.unit.map_or(ctx.worktree, |mine| mine.into);
    Absence::InRunTree {
        path: rendered,
        run_tree: run_tree.to_path_buf(),
        branched_from: (ctx.manifest.isolation == yunta_core::Isolation::Worktree)
            .then(|| ctx.manifest.base_commit.clone()),
    }
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

/// The bytes of the artifact a `context: [{ artifact }]` source names,
/// as the run holds them.
///
/// A reference that names a node asks for that node's artifact and
/// reaches nothing else, whatever another node wrote under the same
/// name; one that names none asks about the run, and gets the acceptance
/// standing last.
pub(super) async fn resolve_artifact(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    artifact: &yunta_core::ArtifactContextRef,
) -> Result<Vec<u8>, ContextResolveError> {
    let wanted = yunta_core::events::ArtifactId::from(&artifact.id);
    let missing = || ContextResolveError::MissingArtifact {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        referenced: artifact.node.clone(),
        artifact: wanted.clone(),
    };
    let events = ctx
        .load_events()
        .await
        .map_err(|e| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: "read the event log".to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
    let held = crate::artifacts::RunArtifacts::of(ctx.run_dir, &events);
    let found = held
        .held(&wanted, artifact.node.as_ref())
        .ok_or_else(missing)?;
    held.bytes(found)
        .await
        .map_err(|source| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: format!(
                "read the artifact `{}` the run holds",
                crate::artifacts::describe(found)
            ),
            source: std::io::Error::other(source.to_string()),
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
            .filter(|e| matches!(e.payload(), Some(EventPayload::Node(NodeEvent::Failed(_)))))
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
                        EventPayload::Findings(FindingEvent::Posted(_))
                            | EventPayload::Findings(FindingEvent::Updated(_))
                            | EventPayload::Findings(FindingEvent::Withdrawn(_))
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

pub(super) async fn resolve_tasks(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
) -> Result<Vec<u8>, ContextResolveError> {
    // A storage failure is not an empty tasks document: handing the agent "no
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
    tokio::fs::read(&path)
        .await
        .map_err(|_| ContextResolveError::MissingNodeOutput {
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
pub(in crate::run) async fn write_node_output(
    run_dir: &Path,
    node_id: &NodeId,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), RunError> {
    let path = node_output_path(run_dir, node_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| RunError::Io {
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
    tokio::fs::write(&path, content)
        .await
        .map_err(|source| RunError::Io {
            context: format!("write captured output for node `{node_id}`"),
            source,
        })
}

/// Puts a resolved source's bytes where the run keeps every artifact's
/// bytes, and answers with the object and its hash.
///
/// The same store, not a second one: a context segment and an artifact
/// are both content the run must be able to hand back exactly as it
/// recorded it, and one content-addressed store answers for both. What
/// `context_assembled` carries is that hash.
pub(super) async fn materialize(
    run_dir: &Path,
    content: &[u8],
) -> std::io::Result<(PathBuf, ContentHash)> {
    let store = ObjectStore::at(run_dir);
    let hash = store.put(content).await?;
    let path = store.path_of(&hash);
    Ok((path, hash))
}

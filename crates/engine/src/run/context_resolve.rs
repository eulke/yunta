//! Context resolution (§9, T6.1): every `context:` entry is resolved and
//! materialized under `context/<content_hash>/` *before* the node's
//! session opens, then folded into the rendered prompt — so replay can
//! name exactly what a session saw without re-running anything (§9's own
//! "el hash identifica, no sustituye").
//!
//! Scope of this recorte, deliberate and documented in
//! `docs/m0-status.md`'s T6.1 entry rather than left silent:
//! - The seven non-`mcp` builtins (`mcp` is T6.2).
//! - Resolved for `kind: prompt` nodes only — `check` (T6.1) rejects
//!   `context:` on any other kind. A loop node's own per-task sessions
//!   don't go through `execute_prompt` at all (`run_task`'s own dispatch,
//!   T5.2); task-scoped context is a separate integration this recorte
//!   doesn't cover.
//! - `files:` resolves each entry as a literal path (after template
//!   rendering), never a filesystem glob walk — the Contrato's own
//!   example uses two literal paths, and no ✓ of T6.1 exercises pattern
//!   expansion; real glob support is debt, not silently approximated.
//! - `knowledge:` resolves the `repo` layer only — T6.5 adds real
//!   `repo > user > org` precedence. Requesting any other layer is a
//!   typed error, never a silent empty result.
//! - `node-output:` only ever has something to read for `kind: bash`
//!   nodes (`execute_bash` is the only place this module captures output
//!   from, right after the process exits, success or failure alike —
//!   exactly the lint→fix-lint→lint case §11.2 describes). `executor`
//!   node output capture is real debt, not yet wired.
//! - No literal `trait ContextSource` — the Contrato names one, but with
//!   a single set of builtins and no second implementer (packs are M11),
//!   a trait object buys nothing CLAUDE.md would call a real boundary.
//!   Every builtin is a plain resolver function behind one `match`; nothing
//!   here stops a future dynamic-dispatch version once a pack actually
//!   needs to plug in its own source.
//! - Stable-first ordering/serialization (§9.1) is T6.4's job: sources
//!   are assembled in declaration order, and `context_assembled`'s own
//!   `segment_hashes` stays empty until stability classification exists.

use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::events::{ContextAssembledPayload, ContextSourceRef, EventPayload};
use yunta_core::{sha256_hex, ContextSpec, Node, NodeId};

use crate::template::render_template;

use super::node_exec::{fail, template_vars, NodeEnd};
use super::{RunCtx, RunError};

/// Below this many materialized bytes, a source's content is embedded
/// directly in the prompt; at or above it, only a pointer to the
/// materialized file is. No config knob exists for this yet (§9 calls it
/// "umbral configurable" without a number) — the same treatment T5.11
/// gave `MAX_EXPANSION_FILES`.
const INLINE_THRESHOLD_BYTES: usize = 4096;

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
    #[error("context `{source_id}` on node `{node}`: {detail}")]
    Template {
        node: NodeId,
        source_id: String,
        detail: String,
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
        "context `{source_id}` on node `{node}`: node `{referenced}`'s artifact `{name}` was \
         never produced — declare an implicit dependency isn't enough if the node itself never \
         wrote it"
    )]
    MissingArtifact {
        node: NodeId,
        source_id: String,
        referenced: NodeId,
        name: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: node `{referenced}` has no captured output \
         (only `kind: bash` nodes capture output in this recorte)"
    )]
    MissingNodeOutput {
        node: NodeId,
        source_id: String,
        referenced: NodeId,
    },
    #[error("context `{source_id}` on node `{node}`: unsupported run-events filter `{filter}`")]
    UnsupportedFilter {
        node: NodeId,
        source_id: String,
        filter: String,
    },
    #[error(
        "context `{source_id}` on node `{node}`: knowledge layer `{layer}` isn't resolvable yet \
         (T6.5) — only `repo` is"
    )]
    UnsupportedKnowledgeLayer {
        node: NodeId,
        source_id: String,
        layer: String,
    },
}

/// Resolves every `context:` entry on `node`, materializes each, emits
/// one `context_assembled` event, and returns the block of text to
/// prepend to the node's rendered prompt — `None` when the node declares
/// no context at all, so callers never prepend an empty header. A
/// resolution failure is the node's own failure (§9: "una fuente que
/// falla es fallo del nodo"), routed through the same `fail` every other
/// node-level error already uses — never a `RunError`.
pub(super) async fn resolve_and_assemble(
    ctx: &RunCtx<'_>,
    node: &Node,
) -> Result<Result<Option<String>, NodeEnd>, RunError> {
    if node.context.is_empty() {
        return Ok(Ok(None));
    }

    match resolve_all(ctx, node).await {
        Ok(block) => Ok(Ok(Some(block))),
        Err(error) => {
            let end = fail(ctx, node, error.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

async fn resolve_all(ctx: &RunCtx<'_>, node: &Node) -> Result<String, ContextResolveError> {
    let mut blocks = Vec::with_capacity(node.context.len());
    let mut sources = Vec::with_capacity(node.context.len());

    for spec in &node.context {
        let source_id = source_id_for(spec);
        let kind = kind_name(spec);
        let content = resolve_one(ctx, node, &source_id, spec).await?;
        let (path, content_hash) =
            materialize(ctx.run_dir, &content).map_err(|source| ContextResolveError::Io {
                node: node.id.clone(),
                source_id: source_id.clone(),
                action: "materialize resolved context".to_string(),
                source,
            })?;

        blocks.push(render_block(&source_id, kind, &content, &path));
        sources.push(ContextSourceRef {
            source_id,
            kind: kind.to_string(),
            content_hash,
        });
    }

    ctx.emit(
        Some(&node.id),
        EventPayload::ContextAssembled(ContextAssembledPayload {
            sources,
            // §9.1/T6.4: stability classification doesn't exist yet —
            // populated once it does, never guessed at here.
            segment_hashes: std::collections::HashMap::new(),
        }),
    )
    .map_err(|source| ContextResolveError::Io {
        node: node.id.clone(),
        source_id: "*".to_string(),
        action: "record context_assembled".to_string(),
        source: std::io::Error::other(source.to_string()),
    })?;

    Ok(blocks.join("\n"))
}

async fn resolve_one(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    spec: &ContextSpec,
) -> Result<Vec<u8>, ContextResolveError> {
    match spec {
        ContextSpec::Files { files } => resolve_files(ctx, node, source_id, files).await,
        ContextSpec::Command { command } => resolve_command(ctx, node, source_id, command).await,
        ContextSpec::Artifact { artifact } => {
            resolve_artifact(ctx, node, source_id, artifact).await
        }
        ContextSpec::RunEvents { run_events } => {
            resolve_run_events(ctx, node, source_id, run_events).await
        }
        ContextSpec::Ledger { .. } => resolve_ledger(ctx).await,
        ContextSpec::Knowledge { knowledge } => {
            resolve_knowledge(ctx, node, source_id, knowledge).await
        }
        ContextSpec::NodeOutput { node_output } => {
            resolve_node_output(ctx, node, source_id, node_output).await
        }
    }
}

async fn resolve_files(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    files: &[String],
) -> Result<Vec<u8>, ContextResolveError> {
    let vars = template_vars(ctx);
    let mut out = Vec::new();
    for pattern in files {
        let rendered =
            render_template(pattern, &vars).map_err(|e| ContextResolveError::Template {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                detail: e.to_string(),
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

async fn resolve_command(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    command: &str,
) -> Result<Vec<u8>, ContextResolveError> {
    let vars = template_vars(ctx);
    let rendered = render_template(command, &vars).map_err(|e| ContextResolveError::Template {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        detail: e.to_string(),
    })?;
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .current_dir(ctx.worktree)
        .output()
        .await
        .map_err(|source| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: format!("run `{rendered}`"),
            source,
        })?;
    if !output.status.success() {
        return Err(ContextResolveError::CommandFailed {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            cmd: rendered,
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
}

async fn resolve_artifact(
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

async fn resolve_run_events(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::RunEventsParams,
) -> Result<Vec<u8>, ContextResolveError> {
    let events = ctx.load_events().map_err(|e| ContextResolveError::Io {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        action: "read the event log".to_string(),
        source: std::io::Error::other(e.to_string()),
    })?;
    let lines: Vec<String> = match params.filter.as_deref() {
        None => events.iter().map(|e| format!("{e:?}")).collect(),
        Some("failed") => events
            .iter()
            .filter(|e| matches!(&e.payload, EventPayload::NodeFailed(_)))
            .map(|e| format!("{e:?}"))
            .collect(),
        Some(other) => {
            return Err(ContextResolveError::UnsupportedFilter {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                filter: other.to_string(),
            })
        }
    };
    Ok(lines.join("\n").into_bytes())
}

async fn resolve_ledger(ctx: &RunCtx<'_>) -> Result<Vec<u8>, ContextResolveError> {
    let events = ctx.load_events().unwrap_or_default();
    let state = crate::replay::derive(&events);
    let mut lines: Vec<String> = state
        .tasks
        .iter()
        .map(|(id, status)| format!("{id}: {status:?}"))
        .collect();
    lines.sort();
    Ok(lines.join("\n").into_bytes())
}

async fn resolve_knowledge(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::KnowledgeParams,
) -> Result<Vec<u8>, ContextResolveError> {
    if let Some(layer) = params.layers.iter().find(|l| l.as_str() != "repo") {
        return Err(ContextResolveError::UnsupportedKnowledgeLayer {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            layer: layer.clone(),
        });
    }

    let dir = ctx.worktree.join(".yunta").join("knowledge");
    if !dir.exists() {
        // No local override is the ordinary case for a fresh repo, not a
        // broken source — distinct from a genuinely missing artifact or
        // node output, which always mean something the workflow expected
        // to exist doesn't.
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|source| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: format!("list `{}`", dir.display()),
            source,
        })?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file())
        .collect();
    entries.sort();
    for path in entries {
        let bytes = std::fs::read(&path).map_err(|source| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: format!("read `{}`", path.display()),
            source,
        })?;
        out.extend_from_slice(format!("# {}\n", path.display()).as_bytes());
        out.extend_from_slice(&bytes);
        out.push(b'\n');
    }
    Ok(out)
}

async fn resolve_node_output(
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

/// Captures a `kind: bash` node's own stdout/stderr right after it exits
/// (§9/§11.2) — called regardless of exit status, since a *failing*
/// node's output is exactly what a corrective node's `node-output`
/// context wants to read.
pub(super) fn write_node_output(
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

fn materialize(run_dir: &Path, content: &[u8]) -> std::io::Result<(PathBuf, String)> {
    let hash = sha256_hex(content);
    let dir = run_dir.join("context").join(&hash);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("content");
    std::fs::write(&path, content)?;
    Ok((path, hash))
}

fn render_block(source_id: &str, kind: &str, content: &[u8], materialized_path: &Path) -> String {
    if content.len() <= INLINE_THRESHOLD_BYTES {
        format!(
            "--- context: {source_id} ({kind}) ---\n{}",
            String::from_utf8_lossy(content)
        )
    } else {
        format!(
            "--- context: {source_id} ({kind}) ---\n[{} bytes, referenced — see {}]\n",
            content.len(),
            materialized_path.display()
        )
    }
}

fn source_id_for(spec: &ContextSpec) -> String {
    match spec {
        ContextSpec::Files { files } => format!("files:{}", files.join(",")),
        ContextSpec::Command { command } => format!("command:{command}"),
        ContextSpec::Artifact { artifact } => {
            format!("artifact:{}/{}", artifact.node, artifact.name)
        }
        ContextSpec::RunEvents { run_events } => format!(
            "run-events:{}",
            run_events.filter.as_deref().unwrap_or("all")
        ),
        ContextSpec::Ledger { .. } => "ledger".to_string(),
        ContextSpec::Knowledge { knowledge } => {
            if knowledge.layers.is_empty() {
                "knowledge:all".to_string()
            } else {
                format!("knowledge:{}", knowledge.layers.join(","))
            }
        }
        ContextSpec::NodeOutput { node_output } => format!("node-output:{}", node_output.node),
    }
}

fn kind_name(spec: &ContextSpec) -> &'static str {
    match spec {
        ContextSpec::Files { .. } => "files",
        ContextSpec::Command { .. } => "command",
        ContextSpec::Artifact { .. } => "artifact",
        ContextSpec::RunEvents { .. } => "run-events",
        ContextSpec::Ledger { .. } => "ledger",
        ContextSpec::Knowledge { .. } => "knowledge",
        ContextSpec::NodeOutput { .. } => "node-output",
    }
}

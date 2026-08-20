//! Context resolution (§9, T6.1): every `context:` entry is resolved and
//! materialized under `context/<content_hash>/` *before* the node's
//! session opens, then folded into the rendered prompt — so replay can
//! name exactly what a session saw without re-running anything (§9's own
//! "el hash identifica, no sustituye").
//!
//! Scope of this recorte, deliberate and documented in
//! `docs/m0-status.md`'s T6.1/T6.2 entries rather than left silent:
//! - Resolved for `kind: prompt` nodes only — `check` (T6.1) rejects
//!   `context:` on any other kind. A loop node's own per-task sessions
//!   don't go through `execute_prompt` at all (`run_task`'s own dispatch,
//!   T5.2); task-scoped context is a separate integration this recorte
//!   doesn't cover.
//! - `files:` resolves each entry as a literal path (after template
//!   rendering), never a filesystem glob walk — the Contrato's own
//!   example uses two literal paths, and no ✓ of T6.1 exercises pattern
//!   expansion; real glob support is debt, not silently approximated.
//! - `knowledge:` (T6.5) resolves `repo` and `user` with real precedence
//!   (§9.2): the two layers merge by filename, `repo` overwriting `user`
//!   on a name collision, regardless of the order `layers:` names them
//!   in. `org` is a legitimate schema value (a versioned pack,
//!   RFC-0002) but has no resolver — packs land in M11 — so requesting
//!   it is a typed error, never a silent empty result.
//! - `node-output:` only ever has something to read for `kind: bash`
//!   nodes (`execute_bash` is the only place this module captures output
//!   from, right after the process exits, success or failure alike —
//!   exactly the lint→fix-lint→lint case §11.2 describes). `executor`
//!   node output capture is real debt, not yet wired.
//! - Stable-first assembly (§9.1, T6.4): every source is classified
//!   `stable | run-stable | volatile` (`stability_class`, straight from
//!   §9.1's own examples) and the final text is always segment-ordered
//!   that way, regardless of `context:`'s own declaration order — the
//!   ordering a provider's prompt cache needs a byte-stable prefix to
//!   help at all. `context_assembled.segment_hashes` carries one hash
//!   per non-empty class, over exactly that class's own canonical text.
//! - `mcp:` (T6.2) speaks streamable-HTTP only, matching the reference
//!   config's own `mcp_servers:` shape (`{ url, auth_env }` — a bearer
//!   token's env var *name*, never the token itself, I12/O3). No stdio
//!   MCP transport exists here; `mcp_servers:` never declares a launch
//!   command, only a URL, so there is nothing to spawn.  The Contrato
//!   fixes neither which MCP verb `query:` maps to nor a tool name —
//!   resolved as a `tools/call` on a tool literally named `query`, the
//!   simplest reading of the field's own name; the toy server this
//!   recorte's own tests spawn implements exactly that tool.
//! - No literal `trait ContextSource` — the Contrato names one, but with
//!   a single set of builtins and no second implementer (packs are M11),
//!   a trait object buys nothing CLAUDE.md would call a real boundary.
//!   Every builtin is a plain resolver function behind one `match`;
//!   nothing here stops a future dynamic-dispatch version once a pack
//!   actually needs to plug in its own source.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rmcp::model::CallToolRequestParams;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use thiserror::Error;
use yunta_core::events::{ContextAssembledPayload, ContextSourceRef, EventPayload};
use yunta_core::{sha256_hex, ContextSpec, Node, NodeId};

use crate::template::render_template;

use super::node_exec::{fail, template_vars, NodeEnd};
use super::{RunCtx, RunError};

/// Bound on how long any single external call (`command:`'s subprocess,
/// `mcp:`'s round trip) may run before this recorte gives up and fails
/// the node — §9 says "command: stdout con timeout" but names no number;
/// same "no number in §9" treatment T5.11 gave `MAX_EXPANSION_FILES`.
const EXTERNAL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

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
        "context `{source_id}` on node `{node}`: command `{cmd}` did not finish within {}s",
        EXTERNAL_CALL_TIMEOUT.as_secs()
    )]
    CommandTimedOut {
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
        /// `None` for the node-less form (D108): the read is against
        /// this run's own `artifacts/`, producer unnamed on purpose.
        referenced: Option<NodeId>,
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
        "context `{source_id}` on node `{node}`: knowledge layer `{layer}` has no resolver yet \
         — the `org` layer is a versioned pack (RFC-0002), and packs land in M11"
    )]
    UnsupportedKnowledgeLayer {
        node: NodeId,
        source_id: String,
        layer: yunta_core::KnowledgeLayer,
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
    #[error("context `{source_id}` on node `{node}`: mcp server `{server}`: {detail}")]
    McpFailed {
        node: NodeId,
        source_id: String,
        server: String,
        detail: String,
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

    match resolve_all(ctx, node, None, None).await {
        Ok(block) => Ok(Ok(Some(block))),
        Err(error) => {
            let end = fail(ctx, node, error.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

/// DI-17: resolved content cached across one loop node's task briefs,
/// for the classes that cannot change within a run — `stable` (repo
/// files, knowledge) and `run-stable` (frozen artifacts, I3). Volatile
/// sources (`command`, `run-events`, `ledger`, `node-output`, `mcp`)
/// re-resolve for every brief, which is the whole reason they're a
/// class of their own (§9.1/D42).
#[derive(Default)]
pub(super) struct StableContextMemo {
    cache: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

/// DI-17: one task brief's context — the same resolution, materialization
/// and `context_assembled` audit a `prompt` node gets, keyed to the task
/// (`task_id` in the event) and memoizing stable sources across briefs.
/// Same error contract as [`resolve_and_assemble`]: a failing source is
/// the node's own failure (§9), never a `RunError`.
pub(super) async fn resolve_for_task(
    ctx: &RunCtx<'_>,
    node: &Node,
    task_id: &yunta_core::TaskId,
    memo: &StableContextMemo,
) -> Result<Result<Option<String>, NodeEnd>, RunError> {
    if node.context.is_empty() {
        return Ok(Ok(None));
    }

    match resolve_all(ctx, node, Some(task_id), Some(memo)).await {
        Ok(block) => Ok(Ok(Some(block))),
        Err(error) => {
            let end = fail(ctx, node, error.to_string(), false)?;
            Ok(Err(end))
        }
    }
}

async fn resolve_all(
    ctx: &RunCtx<'_>,
    node: &Node,
    task_id: Option<&yunta_core::TaskId>,
    memo: Option<&StableContextMemo>,
) -> Result<String, ContextResolveError> {
    let mut sources = Vec::with_capacity(node.context.len());
    let mut stable_blocks = Vec::new();
    let mut run_stable_blocks = Vec::new();
    let mut volatile_blocks = Vec::new();

    for spec in &node.context {
        let source_id = source_id_for(spec);
        let kind = kind_name(spec);
        // A stable/run-stable source already resolved for an earlier
        // brief serves from the memo — same bytes by its own class's
        // definition, so re-reading would only cost IO.
        let memoizable = memo.filter(|_| stability_class(spec) != StabilityClass::Volatile);
        let cached = memoizable.and_then(|memo| {
            memo.cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&source_id)
                .cloned()
        });
        let content = match cached {
            Some(content) => content,
            None => {
                let content = resolve_one(ctx, node, &source_id, spec).await?;
                if let Some(memo) = memoizable {
                    memo.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(source_id.clone(), content.clone());
                }
                content
            }
        };
        let (path, content_hash) =
            materialize(ctx.run_dir, &content).map_err(|source| ContextResolveError::Io {
                node: node.id.clone(),
                source_id: source_id.clone(),
                action: "materialize resolved context".to_string(),
                source,
            })?;

        // §9's "umbral configurable" (DI-05): `limits.inline_context_bytes`,
        // reference default 32000 — the resolved value lives in
        // `ConfigLayer`, never re-invented here.
        let inline_threshold = ctx.manifest.config.resolved_inline_context_bytes() as usize;
        let block = render_block(&source_id, kind, &content, &path, inline_threshold);
        match stability_class(spec) {
            StabilityClass::Stable => stable_blocks.push(block),
            StabilityClass::RunStable => run_stable_blocks.push(block),
            StabilityClass::Volatile => volatile_blocks.push(block),
        }
        sources.push(ContextSourceRef {
            source_id,
            kind: kind.to_string(),
            content_hash,
        });
    }

    // §9.1/T6.4: always assembled stable → run-stable → volatile,
    // regardless of `context:`'s own declaration order — the ordering a
    // provider's prompt cache needs a byte-stable prefix to actually
    // help. Each non-empty class's own canonical text (same order,
    // same separators, every time) gets its own `segment_hashes` entry
    // — comparing that hash across sessions is the mechanical check
    // that the prefix really held.
    let mut segment_hashes = std::collections::HashMap::new();
    let mut assembled = Vec::new();
    for (key, class_blocks) in [
        ("stable", &stable_blocks),
        ("run-stable", &run_stable_blocks),
        ("volatile", &volatile_blocks),
    ] {
        if class_blocks.is_empty() {
            continue;
        }
        let segment_text = class_blocks.join("\n");
        segment_hashes.insert(key.to_string(), sha256_hex(segment_text.as_bytes()));
        assembled.push(segment_text);
    }

    ctx.emit(
        Some(&node.id),
        EventPayload::ContextAssembled(ContextAssembledPayload {
            task_id: task_id.cloned(),
            sources,
            segment_hashes,
        }),
    )
    .map_err(|source| ContextResolveError::Io {
        node: node.id.clone(),
        source_id: "*".to_string(),
        action: "record context_assembled".to_string(),
        source: std::io::Error::other(source.to_string()),
    })?;

    Ok(assembled.join("\n"))
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
        ContextSpec::Mcp { mcp } => resolve_mcp(ctx, node, source_id, mcp).await,
    }
}

async fn resolve_files(
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
    let vars = template_vars(ctx, node);
    let rendered = render_template(command, &vars).map_err(|e| ContextResolveError::Template {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        detail: e.to_string(),
    })?;
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .current_dir(ctx.worktree)
        .output();
    let output = tokio::time::timeout(EXTERNAL_CALL_TIMEOUT, child)
        .await
        .map_err(|_elapsed| ContextResolveError::CommandTimedOut {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            cmd: rendered.clone(),
        })?
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

/// Fixed precedence order (§9.2): `repo` last, so it overwrites `user` by
/// filename when both declare the same doc — never the order `layers:`
/// happens to name them in.
const KNOWLEDGE_PRECEDENCE: [yunta_core::KnowledgeLayer; 2] = [
    yunta_core::KnowledgeLayer::User,
    yunta_core::KnowledgeLayer::Repo,
];

fn knowledge_dir(ctx: &RunCtx<'_>, layer: yunta_core::KnowledgeLayer) -> Option<PathBuf> {
    match layer {
        yunta_core::KnowledgeLayer::Repo => Some(ctx.worktree.join(".yunta").join("knowledge")),
        yunta_core::KnowledgeLayer::User => {
            yunta_core::user_state_root().map(|root| root.join("knowledge"))
        }
        yunta_core::KnowledgeLayer::Org => None,
    }
}

fn list_knowledge_files(
    dir: &Path,
    node: &NodeId,
    source_id: &str,
) -> Result<Vec<PathBuf>, ContextResolveError> {
    if !dir.exists() {
        // No override at this layer is the ordinary case (a fresh repo,
        // or no user-level knowledge yet), not a broken source — distinct
        // from a genuinely missing artifact or node output, which always
        // mean something the workflow expected to exist doesn't.
        return Ok(Vec::new());
    }
    // Recursive since DI-24: the distilled subtree
    // (`distilled/<workflow>/<run>/…`) is part of the layer — §9.2's own
    // "lo destilado acá" — so a flat listing would silently hide exactly
    // the knowledge the close deposited.
    let mut entries: Vec<PathBuf> = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let listing = std::fs::read_dir(&current).map_err(|source| ContextResolveError::Io {
            node: node.clone(),
            source_id: source_id.to_string(),
            action: format!("list `{}`", current.display()),
            source,
        })?;
        for entry in listing.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                entries.push(path);
            }
        }
    }
    entries.sort();
    Ok(entries)
}

async fn resolve_knowledge(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::KnowledgeParams,
) -> Result<Vec<u8>, ContextResolveError> {
    let requested: Vec<yunta_core::KnowledgeLayer> = if params.layers.is_empty() {
        KNOWLEDGE_PRECEDENCE.to_vec()
    } else {
        params.layers.clone()
    };

    if let Some(layer) = requested
        .iter()
        .find(|l| **l == yunta_core::KnowledgeLayer::Org)
    {
        return Err(ContextResolveError::UnsupportedKnowledgeLayer {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            layer: *layer,
        });
    }

    // Precedence (§9.2): "lo del repo pisa a lo general ante conflicto" —
    // a later layer in KNOWLEDGE_PRECEDENCE overwrites an earlier one by
    // filename, so the same doc name in both layers resolves to exactly
    // one copy, never two.
    let mut by_name: std::collections::BTreeMap<std::ffi::OsString, PathBuf> =
        std::collections::BTreeMap::new();
    for layer in KNOWLEDGE_PRECEDENCE {
        if !requested.contains(&layer) {
            continue;
        }
        let Some(dir) = knowledge_dir(ctx, layer) else {
            continue;
        };
        for path in list_knowledge_files(&dir, &node.id, source_id)? {
            if let Some(name) = path.file_name() {
                by_name.insert(name.to_owned(), path);
            }
        }
    }

    let mut out = Vec::new();
    for path in by_name.into_values() {
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

/// `mcp: { server, query }` (§9, T6.2): looks `server` up in the merged
/// config's `mcp_servers:`, connects over streamable-HTTP (bearer token
/// read from the env var `auth_env` names, never from config itself),
/// and calls a tool literally named `query` with the rendered `query:`
/// text as its sole argument — see the module doc for why that specific
/// mapping. The whole round trip (connect, handshake, call) is bounded
/// by `EXTERNAL_CALL_TIMEOUT`; the connection is always closed before
/// returning, success or failure alike.
async fn resolve_mcp(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::McpQueryParams,
) -> Result<Vec<u8>, ContextResolveError> {
    let server = ctx
        .manifest
        .config
        .mcp_servers
        .as_ref()
        .and_then(|servers| servers.get(&params.server))
        .ok_or_else(|| ContextResolveError::UnknownMcpServer {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: params.server.clone(),
        })?;

    let vars = template_vars(ctx, node);
    let query =
        render_template(&params.query, &vars).map_err(|e| ContextResolveError::Template {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            detail: e.to_string(),
        })?;

    let auth_header = match &server.auth_env {
        Some(var) => Some(
            std::env::var(var).map_err(|_| ContextResolveError::MissingAuthEnv {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                server: params.server.clone(),
                var: var.clone(),
            })?,
        ),
        None => None,
    };

    let call = call_mcp_query(server.url.clone(), auth_header, query);
    let text = tokio::time::timeout(EXTERNAL_CALL_TIMEOUT, call)
        .await
        .map_err(|_elapsed| ContextResolveError::McpTimedOut {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: params.server.clone(),
        })?
        .map_err(|detail| ContextResolveError::McpFailed {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: params.server.clone(),
            detail,
        })?;
    Ok(text.into_bytes())
}

/// Connects, calls the `query` tool once, and disconnects — isolated from
/// `resolve_mcp` only so the `?`-heavy rmcp error plumbing collapses to a
/// single `String` before it meets `ContextResolveError`.
async fn call_mcp_query(
    url: String,
    auth_header: Option<String>,
    query: String,
) -> Result<String, String> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    if let Some(token) = auth_header {
        config = config.auth_header(token);
    }
    let transport = StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
    let client = ().serve(transport).await.map_err(|e| e.to_string())?;

    let mut arguments = rmcp::model::JsonObject::new();
    arguments.insert("query".to_string(), serde_json::Value::String(query));
    let result = client
        .call_tool_once(CallToolRequestParams::new("query").with_arguments(arguments))
        .await;
    let _ = client.cancel().await;

    match result.map_err(|e| e.to_string())? {
        rmcp::model::CallToolResponse::Complete(result) => {
            let text: String = result
                .content
                .into_iter()
                .filter_map(|block| match block {
                    rmcp::model::ContentBlock::Text(t) => Some(t.text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if result.is_error == Some(true) {
                Err(format!("tool `query` returned an error: {text}"))
            } else {
                Ok(text)
            }
        }
        other => Err(format!("unexpected tools/call response: {other:?}")),
    }
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

/// Below `inline_threshold` materialized bytes, a source's content is
/// embedded directly in the prompt; at or above it, only a pointer to
/// the materialized file is.
fn render_block(
    source_id: &str,
    kind: &str,
    content: &[u8],
    materialized_path: &Path,
    inline_threshold: usize,
) -> String {
    if content.len() <= inline_threshold {
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

/// §9.1's own three fixed classes, in assembly order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StabilityClass {
    Stable,
    RunStable,
    Volatile,
}

/// §9.1's own examples, applied literally: `files`/`knowledge` are
/// "archivos del repo que el run no toca"/"conocimiento durable" —
/// `stable`; `artifact` is "artifacts congelados: brief, plan" —
/// `run-stable` (I3 already makes every artifact immutable once
/// written, so this is the class its own guarantee already earns);
/// `command`/`run-events`/`node-output`/the aggregate `ledger` view are
/// named `volatile` verbatim. `mcp` isn't in any of §9.1's own lists —
/// classified `volatile` here since a live external server's response
/// is never something this recorte can promise is byte-stable between
/// sessions.
fn stability_class(spec: &ContextSpec) -> StabilityClass {
    match spec {
        ContextSpec::Files { .. } | ContextSpec::Knowledge { .. } => StabilityClass::Stable,
        ContextSpec::Artifact { .. } => StabilityClass::RunStable,
        ContextSpec::Command { .. }
        | ContextSpec::RunEvents { .. }
        | ContextSpec::Ledger { .. }
        | ContextSpec::NodeOutput { .. }
        | ContextSpec::Mcp { .. } => StabilityClass::Volatile,
    }
}

fn source_id_for(spec: &ContextSpec) -> String {
    match spec {
        ContextSpec::Files { files } => format!("files:{}", files.join(",")),
        ContextSpec::Command { command } => format!("command:{command}"),
        ContextSpec::Artifact { artifact } => match &artifact.node {
            Some(node) => format!("artifact:{}/{}", node, artifact.name),
            None => format!("artifact:{}", artifact.name),
        },
        ContextSpec::RunEvents { run_events } => format!(
            "run-events:{}",
            run_events.filter.as_deref().unwrap_or("all")
        ),
        ContextSpec::Ledger { .. } => "ledger".to_string(),
        ContextSpec::Knowledge { knowledge } => {
            if knowledge.layers.is_empty() {
                "knowledge:all".to_string()
            } else {
                let layers: Vec<String> = knowledge.layers.iter().map(|l| l.to_string()).collect();
                format!("knowledge:{}", layers.join(","))
            }
        }
        ContextSpec::NodeOutput { node_output } => format!("node-output:{}", node_output.node),
        ContextSpec::Mcp { mcp } => format!("mcp:{}/{}", mcp.server, mcp.query),
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
        ContextSpec::Mcp { .. } => "mcp",
    }
}

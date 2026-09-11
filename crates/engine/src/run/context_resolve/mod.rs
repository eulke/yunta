//! Context resolution: every `context:` entry is resolved and
//! materialized under `context/<content_hash>/` *before* the node's
//! session opens, then folded into the rendered prompt — so replay can
//! name exactly what a session saw without re-running anything (the
//! hash identifies the content, it never substitutes for it).
//!
//! Scope of this module, deliberate and documented rather than left
//! silent:
//! - Resolved for `kind: prompt` nodes only — validation rejects
//!   `context:` on any other kind. A loop node's own per-task sessions
//!   don't go through `execute_prompt` at all (`run_task`'s own
//!   dispatch); task-scoped context is a separate integration this
//!   module doesn't cover.
//! - `files:` resolves each entry as a literal path (after template
//!   rendering), never a filesystem glob walk — no test here exercises
//!   pattern expansion; real glob support is debt, not silently
//!   approximated.
//! - `knowledge:` resolves all three layers with real precedence: the
//!   layers merge by filename, `repo` overwriting `user` overwriting
//!   `org` on a name collision, regardless of the order `layers:` names
//!   them in. `org` is the union of every installed knowledge pack's
//!   declared contents (RFC-0002 vendoring, via the pack catalog); two
//!   *packs* shipping the same filename is a typed error naming both —
//!   between packs there is no precedence to fall back on.
//! - `node-output:` only ever has something to read for `kind: bash`
//!   nodes (`execute_bash` is the only place this module captures output
//!   from, right after the process exits, success or failure alike —
//!   exactly the lint→fix-lint→lint case a corrective node needs to
//!   read back). An `executor` node's output is not captured for it:
//!   registered debt A-11 in `docs/design/deuda-consciente.md`.
//! - Stable-first assembly: every source is classified
//!   `stable | run-stable | volatile` (`stability_class`) and the final
//!   text is always segment-ordered that way, regardless of `context:`'s
//!   own declaration order — the ordering a provider's prompt cache
//!   needs a byte-stable prefix to help at all.
//!   `context_assembled.segment_hashes` carries one hash per non-empty
//!   class, over exactly that class's own canonical text.
//! - `mcp:` speaks streamable-HTTP only, matching the reference config's
//!   own `mcp_servers:` shape (`{ url, auth_env }` — a bearer token's
//!   env var *name*, never the token itself, so no secret ever lands in
//!   config or the log). No stdio MCP transport exists here;
//!   `mcp_servers:` never declares a launch command, only a URL, so
//!   there is nothing to spawn. `query:` resolves as a `tools/call` on a
//!   tool literally named `query`, the simplest reading of the field's
//!   own name; the toy server this module's own tests spawn implements
//!   exactly that tool.
//! - No literal `trait ContextSource` — with a single set of builtins
//!   and no second implementer (packs are still ahead), a trait object
//!   buys nothing a real boundary would. Every builtin is a plain
//!   resolver function behind
//!   one `match`; nothing here stops a future dynamic-dispatch version
//!   once a pack actually needs to plug in its own source.

mod error;
mod knowledge;
mod mcp;
mod sources;

use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{ContextAssembledPayload, ContextSourceRef, EventPayload};
use yunta_core::{sha256_hex, ContextSpec, Node};

use super::node_close::fail;
use super::node_exec::cancelled_end;
use super::step::Step;
use super::{RunCtx, RunError};
use error::ContextResolveError;
use knowledge::resolve_knowledge;
use mcp::resolve_mcp;
use sources::{
    materialize, resolve_artifact, resolve_command, resolve_files, resolve_ledger,
    resolve_node_output, resolve_run_events,
};

pub(super) use sources::write_node_output;

/// Bound on how long any single external call (`command:`'s subprocess,
/// `mcp:`'s round trip) may run before the node gives up and fails —
/// the spec requires a timeout on `command:` stdout but names no number,
/// so this constant fixes one.
const EXTERNAL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolves every `context:` entry on `node`, materializes each, emits
/// one `context_assembled` event, and returns the block of text to
/// prepend to the node's rendered prompt — `None` when the node declares
/// no context at all, so callers never prepend an empty header. A
/// resolution failure is the node's own failure ("a source that fails
/// is a failure of the node"), routed through the same `fail` every
/// other node-level error already uses — never a `RunError`. A
/// `command:` source runs under `cancel`: when the token fires the
/// command dies with its tree and the node ends as every cancelled
/// node ends.
pub(super) async fn resolve_and_assemble(
    ctx: &RunCtx<'_>,
    node: &Node,
    cancel: &CancellationToken,
) -> Result<Step<Option<String>>, RunError> {
    assemble(ctx, node, None, None, cancel).await
}

/// Resolved content cached across one loop node's task briefs,
/// for the classes that cannot change within a run — `stable` (repo
/// files, knowledge) and `run-stable` (frozen artifacts, immutable once
/// written). Volatile sources (`command`, `run-events`, `ledger`,
/// `node-output`, `mcp`) re-resolve for every brief, which is the whole
/// reason they're a class of their own.
#[derive(Default)]
pub(super) struct StableContextMemo {
    cache: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

/// One task brief's context — the same resolution, materialization
/// and `context_assembled` audit a `prompt` node gets, keyed to the task
/// (`task_id` in the event) and memoizing stable sources across briefs.
/// Same error contract as [`resolve_and_assemble`]: a failing source is
/// the node's own failure, never a `RunError`.
pub(super) async fn resolve_for_task(
    ctx: &RunCtx<'_>,
    node: &Node,
    task_id: &yunta_core::TaskId,
    memo: &StableContextMemo,
    cancel: &CancellationToken,
) -> Result<Step<Option<String>>, RunError> {
    assemble(ctx, node, Some(task_id), Some(memo), cancel).await
}

/// The one mapping from a resolution's result to the node's end: a
/// block to prepend, the node's own failure, or — when `cancel` fired
/// while a `command:` source ran — the cancelled end.
async fn assemble(
    ctx: &RunCtx<'_>,
    node: &Node,
    task_id: Option<&yunta_core::TaskId>,
    memo: Option<&StableContextMemo>,
    cancel: &CancellationToken,
) -> Result<Step<Option<String>>, RunError> {
    if node.context.is_empty() && artifact_shapes(ctx, node).is_empty() {
        return Ok(Step::Value(None));
    }

    match resolve_all(ctx, node, task_id, memo, cancel).await {
        Ok(block) => Ok(Step::Value(Some(block))),
        Err(ContextResolveError::Cancelled { .. }) => {
            Ok(Step::Ended(cancelled_end(ctx, node).await?))
        }
        Err(error) => Ok(Step::Ended(
            fail(ctx, node, error.to_string(), false).await?,
        )),
    }
}

async fn resolve_all(
    ctx: &RunCtx<'_>,
    node: &Node,
    task_id: Option<&yunta_core::TaskId>,
    memo: Option<&StableContextMemo>,
    cancel: &CancellationToken,
) -> Result<String, ContextResolveError> {
    let mut sources = Vec::with_capacity(node.context.len());
    let mut stable_blocks = Vec::new();
    let mut run_stable_blocks = Vec::new();
    let mut volatile_blocks = Vec::new();

    // The shape of every interpreted artifact this node declares, ahead
    // of everything the author asked for. It is `stable` by
    // construction — derived from the node's own declaration and the
    // types that parse it, identical in every session of this node — so
    // it sits inside the byte-stable prefix a provider's cache reuses
    // rather than disturbing it.
    for (source_id, content) in artifact_shapes(ctx, node) {
        let bytes = content.into_bytes();
        let (path, content_hash) =
            materialize(ctx.run_dir, &bytes).map_err(|source| ContextResolveError::Io {
                node: node.id.clone(),
                source_id: source_id.clone(),
                action: "materialize an artifact shape".to_string(),
                source,
            })?;
        let inline_threshold = ctx.manifest.config.resolved_inline_context_bytes() as usize;
        stable_blocks.push(render_block(
            &source_id,
            SHAPE_KIND,
            &bytes,
            &path,
            inline_threshold,
        ));
        sources.push(ContextSourceRef {
            source_id,
            kind: SHAPE_KIND.to_string(),
            content_hash,
        });
    }

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
                let content = resolve_one(ctx, node, &source_id, spec, cancel).await?;
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

        // The configurable threshold: `limits.inline_context_bytes`,
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

    // Always assembled stable → run-stable → volatile,
    // regardless of `context:`'s own declaration order — the ordering a
    // provider's prompt cache needs a byte-stable prefix to actually
    // help. Each non-empty class's own canonical text (same order,
    // same separators, every time) gets its own `segment_hashes` entry
    // — comparing that hash across sessions is the mechanical check
    // that the prefix really held.
    let mut segment_hashes = std::collections::BTreeMap::new();
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
    .await
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
    cancel: &CancellationToken,
) -> Result<Vec<u8>, ContextResolveError> {
    match spec {
        ContextSpec::Files { files } => resolve_files(ctx, node, source_id, files).await,
        ContextSpec::Command { command } => {
            resolve_command(ctx, node, source_id, command, cancel).await
        }
        ContextSpec::Artifact { artifact } => {
            resolve_artifact(ctx, node, source_id, artifact).await
        }
        ContextSpec::RunEvents { run_events } => {
            resolve_run_events(ctx, node, source_id, run_events).await
        }
        ContextSpec::Ledger { .. } => resolve_ledger(ctx, node, source_id).await,
        ContextSpec::Knowledge { knowledge } => {
            resolve_knowledge(ctx, node, source_id, knowledge).await
        }
        ContextSpec::NodeOutput { node_output } => {
            resolve_node_output(ctx, node, source_id, node_output).await
        }
        ContextSpec::Mcp { mcp } => resolve_mcp(ctx, node, source_id, mcp).await,
    }
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

/// What a shape block is called wherever context sources are named:
/// in the prompt's own header and in `context_assembled`.
const SHAPE_KIND: &str = "artifact-shape";

/// The shape of every interpreted artifact a node declares, as blocks to
/// mount ahead of the author's own context.
///
/// A node that declared `kind: task-ledger` has already said everything
/// needed to know this: publishing the shape is the consequence of that
/// declaration, not a second key an author has to remember. An opaque
/// artifact yields nothing — it has no shape to demand.
///
/// The path is spelled out because the engine knows it and the session
/// does not: its working directory is the worktree, not the run
/// directory, so an agent told only to "write an artifact" has nowhere
/// to put it.
fn artifact_shapes(ctx: &RunCtx<'_>, node: &Node) -> Vec<(String, String)> {
    // Names carry templates (`findings-{{runner.role}}`); the session is
    // told the name it will actually be verified against. A name that
    // cannot render is the node's own failure at close, reported there
    // with its own diagnostic — here it simply stays as written.
    let rendered = super::node_exec::render_artifact_names(ctx, node);
    let node = rendered.as_ref().unwrap_or(node);
    let Some(artifacts) = &node.artifacts else {
        return Vec::new();
    };
    artifacts
        .produces
        .iter()
        .filter_map(|spec| match spec {
            yunta_core::ArtifactSpec::Typed { name, kind } => {
                let path = ctx.run_dir.join("artifacts").join(name);
                let shape =
                    crate::artifacts::published_shape(yunta_core::DocumentKind::from(kind.clone()));
                Some((
                    format!("{SHAPE_KIND}:{name}"),
                    format!(
                        "This node produces an artifact the engine reads and validates. Write \
                         it at {}, in exactly this shape — any other key fails the \
                         node.\n\n{shape}",
                        path.display()
                    ),
                ))
            }
            yunta_core::ArtifactSpec::Plain(_) => None,
        })
        .collect()
}

/// The three fixed classes, in assembly order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StabilityClass {
    Stable,
    RunStable,
    Volatile,
}

/// Applied literally: `files`/`knowledge` are repo files the run never
/// touches, or durable knowledge — `stable`; `artifact` is a frozen
/// artifact such as a brief or plan — `run-stable` (every artifact is
/// already immutable once written, so this is the class its own
/// guarantee already earns); `command`/`run-events`/`node-output`/the
/// aggregate `ledger` view are `volatile`. `mcp` has no obvious home in
/// that scheme — classified `volatile` here since a live external
/// server's response is never something this module can promise is
/// byte-stable between sessions.
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
            run_events.filter.map(|f| f.as_str()).unwrap_or("all")
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

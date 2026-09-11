//! Publishing the shape of every interpreted artifact a node declares.
//!
//! A node that declared `kind: task-ledger` has already said everything
//! needed to know this: publishing the shape is the consequence of that
//! declaration, not a second key an author has to remember. It is the
//! only context block the engine adds on its own, which is why it lives
//! apart from resolving what the author asked for.
//!
//! Publishing it here, once, is also what keeps it out of the repair
//! instruction: a session that gets the shape in its stable prefix does
//! not need it again appended to the problems its last attempt left.

use yunta_core::events::ContextSourceRef;
use yunta_core::Node;

use crate::run::node_exec::render_artifact_names;
use crate::run::RunCtx;

use super::error::ContextResolveError;
use super::render_block;
use super::sources::materialize;

/// What a shape block is called wherever context sources are named:
/// in the prompt's own header and in `context_assembled`.
pub(super) const SHAPE_KIND: &str = "artifact-shape";

/// Mounts the shape of every interpreted artifact this node declares,
/// ahead of everything the author asked for.
///
/// `stable` by construction rather than by choice: the text derives from
/// the node's own declaration and the types that parse it, so it is
/// identical in every session of this node and sits inside the
/// byte-stable prefix a provider's cache reuses rather than disturbing
/// it.
pub(super) fn mount_artifact_shapes(
    ctx: &RunCtx<'_>,
    node: &Node,
    blocks: &mut Vec<String>,
    sources: &mut Vec<ContextSourceRef>,
) -> Result<(), ContextResolveError> {
    let inline_threshold = ctx.manifest.config.resolved_inline_context_bytes() as usize;
    for (source_id, content) in artifact_shapes(ctx, node) {
        let bytes = content.into_bytes();
        let (path, content_hash) =
            materialize(ctx.run_dir, &bytes).map_err(|source| ContextResolveError::Io {
                node: node.id.clone(),
                source_id: source_id.clone(),
                action: "materialize an artifact shape".to_string(),
                source,
            })?;
        blocks.push(render_block(
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
    Ok(())
}

/// The shape of every interpreted artifact a node declares, as blocks to
/// mount ahead of the author's own context. An opaque artifact yields
/// nothing — it has no shape to demand.
///
/// The path is spelled out because the engine knows it and the session
/// does not: its working directory is the worktree, not the run
/// directory, so an agent told only to "write an artifact" has nowhere
/// to put it.
///
/// Names carry templates (`findings-{{runner.role}}`), and what is
/// published is the rendered name — the one the close will verify
/// against. A node whose names do not render has no such name to give,
/// so it publishes nothing and fails at close with the template error
/// naming the variable; telling a session to write
/// `findings-{{runner.role}}` would only buy a file verification is
/// never going to look for.
pub(super) fn artifact_shapes(ctx: &RunCtx<'_>, node: &Node) -> Vec<(String, String)> {
    let Ok(node) = render_artifact_names(ctx, node) else {
        return Vec::new();
    };
    let Some(artifacts) = &node.artifacts else {
        return Vec::new();
    };
    artifacts
        .produces
        .iter()
        .filter_map(|spec| match spec {
            yunta_core::ArtifactSpec::Typed { name, kind } => {
                let path = ctx.run_dir.join("artifacts").join(name);
                let shape = yunta_core::shape::contract(*kind);
                Some((
                    format!("{SHAPE_KIND}:{name}"),
                    format!(
                        "This node produces an artifact the engine reads and validates. \
                         Write it at {}.\n\nWhat follows is the whole contract for that \
                         file — the keys, their types, and the rules. Where any other \
                         instruction describes this file differently, this is what the \
                         engine enforces.\n\nWhen you have written it, call \
                         `yunta_check_artifact` — it runs this node's own verification \
                         and answers while you can still fix what it names.\n\n{shape}",
                        path.display()
                    ),
                ))
            }
            yunta_core::ArtifactSpec::Plain(_) => None,
        })
        .collect()
}

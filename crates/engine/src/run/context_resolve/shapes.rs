//! Publishing the shape of every interpreted artifact a node declares.
//!
//! A node that declared `kind: tasks` has already said everything
//! needed to know this: publishing the shape is the consequence of that
//! declaration, not a second key an author has to remember. It is the
//! only context block the engine adds on its own, which is why it lives
//! apart from resolving what the author asked for.

use yunta_core::events::ContextSourceRef;
use yunta_core::{ArtifactKind, Node};

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
pub(super) async fn mount_artifact_shapes(
    ctx: &RunCtx<'_>,
    node: &Node,
    naming: yunta_core::ToolNaming,
    (blocks, sources): (&mut Vec<String>, &mut Vec<ContextSourceRef>),
) -> Result<(), ContextResolveError> {
    let inline_threshold = ctx.manifest.config.resolved_inline_context_bytes() as usize;
    let suite = crate::run::baseline::suite(ctx.manifest).and_then(yunta_core::Suite::of);
    for (source_id, content) in artifact_shapes(node, naming, suite.as_ref()) {
        let bytes = content.into_bytes();
        let (path, content_hash) =
            materialize(ctx.run_dir, &bytes)
                .await
                .map_err(|source| ContextResolveError::Io {
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
            absent: Vec::new(),
        });
    }
    Ok(())
}

/// The shape of every interpreted artifact a node declares, as blocks to
/// mount ahead of the author's own context. An opaque artifact yields
/// nothing — it has no shape to demand.
///
/// The block names the kind and what carries it, never a path: an
/// interpreted artifact is a document the session hands over, identified
/// by its kind, and the file is the engine's to write. The tool that
/// carries it is named the way the session's CLI names it (`naming`). A
/// plan or a spec is told the `suite` the run measures, which its commands
/// extend when they run the project's tests.
pub(super) fn artifact_shapes(
    node: &Node,
    naming: yunta_core::ToolNaming,
    suite: Option<&yunta_core::Suite>,
) -> Vec<(String, String)> {
    let Some(artifacts) = &node.artifacts else {
        return Vec::new();
    };
    artifacts
        .produces
        .iter()
        .filter_map(|spec| match spec {
            yunta_core::ArtifactSpec::Interpreted(kind) => {
                let shape = yunta_core::shape::contract(*kind);
                let opening = match kind.submit_tool() {
                    Some(_) => format!(
                        "This node produces the `{kind}` document. It is not a file this \
                         session writes: hand it over with `{tool}`, and the engine \
                         validates it and writes the file itself.",
                        tool = yunta_core::RunTool::Submit(*kind).called(naming),
                    ),
                    None => format!(
                        "This node produces the `{kind}` artifact. It is not a file this \
                         session writes: report each finding through its run tools the \
                         moment you see it, and the engine writes the file at the end from \
                         everything this node reported. A finding reported before this \
                         session ends survives whatever happens after."
                    ),
                };
                let suite = the_suite_it_extends(*kind, suite);
                Some((
                    format!("{SHAPE_KIND}:{kind}"),
                    format!(
                        "{opening}\n\nWhat follows is the whole contract for that \
                         document — the keys, their types, and the rules. Where any other \
                         instruction describes it differently, this is what the engine \
                         enforces.\n\n{shape}{suite}"
                    ),
                ))
            }
            yunta_core::ArtifactSpec::Opaque(_) => None,
        })
        .collect()
}

/// What a plan or a spec is told about the suite the run measures, which
/// its commands extend when they run the project's tests. Nothing for any
/// other document, or a run that measures no suite.
fn the_suite_it_extends(kind: ArtifactKind, suite: Option<&yunta_core::Suite>) -> String {
    match (kind, suite) {
        (ArtifactKind::Tasks | ArtifactKind::Spec, Some(suite)) => format!(
            "\n\nThis run measures its suite with `{command}`. A command of this document \
             that runs the project's tests extends it — `{command}` followed by what selects \
             its tests — so the build the suite leaves serves it; the engine refuses one that \
             runs them another way.",
            command = suite.command()
        ),
        _ => String::new(),
    }
}

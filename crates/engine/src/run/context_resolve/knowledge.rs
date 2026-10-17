//! `knowledge:` — the layered knowledge directories, most to least
//! local, with the org layer assembled from the installed packs. Each
//! layer is read through the run's frozen inputs, so a file edited while
//! the run waits reads as it was when the run was born.

use std::collections::BTreeMap;
use std::ffi::OsString;

use yunta_core::{KnowledgeLayer, Node, NodeId};

use super::error::ContextResolveError;
use crate::frozen::{KnowledgeFile, Source};
use crate::run::RunCtx;

/// The org layer's files, one per filename. Between two installed packs
/// there is no order, so a filename both ship is a typed error naming both;
/// within one pack the later entry wins.
fn org_files(
    files: Vec<KnowledgeFile>,
    node: &NodeId,
    source_id: &str,
) -> Result<Vec<KnowledgeFile>, ContextResolveError> {
    let mut by_name: BTreeMap<OsString, KnowledgeFile> = BTreeMap::new();
    for file in files {
        if let Some(other) = by_name.get(&file.name) {
            if other.pack != file.pack {
                return Err(ContextResolveError::OrgKnowledgeCollision {
                    node: node.clone(),
                    source_id: source_id.to_string(),
                    file: file.name.to_string_lossy().into_owned(),
                    pack_a: other.pack.clone().unwrap_or_default(),
                    pack_b: file.pack.clone().unwrap_or_default(),
                });
            }
        }
        by_name.insert(file.name.clone(), file);
    }
    Ok(by_name.into_values().collect())
}

/// The name a knowledge file is shown under: its layer, then its pack when
/// an org file came from one, then its path within that layer.
fn shown_as(layer: KnowledgeLayer, file: &KnowledgeFile) -> String {
    let relative = &file.relative;
    match &file.pack {
        Some(pack) => format!("{layer}/{pack}/{relative}"),
        None => format!("{layer}/{relative}"),
    }
}

/// The files one layer holds in the run's frozen set. A frozen file that no
/// longer matches its hash refuses the layer; the org layer is also checked
/// for collisions between its packs.
async fn layer_files(
    source: &Source<'_>,
    layer: KnowledgeLayer,
    node: &Node,
    source_id: &str,
) -> Result<Vec<KnowledgeFile>, ContextResolveError> {
    let files = source
        .knowledge(layer)
        .await
        .map_err(|error| ContextResolveError::Frozen {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            source: error,
        })?;
    if layer == KnowledgeLayer::Org {
        org_files(files, &node.id, source_id)
    } else {
        Ok(files)
    }
}

pub(super) async fn resolve_knowledge(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::KnowledgeParams,
) -> Result<Vec<u8>, ContextResolveError> {
    // Precedence: the repo layer overrides the more general ones on
    // conflict — a later layer in `KnowledgeLayer::PRECEDENCE` overwrites an
    // earlier one by filename, so the same doc name in two layers
    // resolves to exactly one copy, never two. Org is the union of
    // installed knowledge packs, collision-checked among themselves
    // first.
    let source = Source::of(ctx.manifest, ctx.run_dir, ctx.worktree, ctx.ambient);
    let mut by_name: BTreeMap<OsString, (KnowledgeLayer, KnowledgeFile)> = BTreeMap::new();
    for layer in params.layers_read() {
        for file in layer_files(&source, layer, node, source_id).await? {
            by_name.insert(file.name.clone(), (layer, file));
        }
    }

    let mut out = Vec::new();
    for (layer, file) in by_name.into_values() {
        out.extend_from_slice(format!("# {}\n", shown_as(layer, &file)).as_bytes());
        out.extend_from_slice(&file.bytes);
        out.push(b'\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// A file of a layer, as the layer's listing reads it.
    fn file(pack: Option<&str>, relative: &str) -> KnowledgeFile {
        KnowledgeFile {
            pack: pack.map(str::to_string),
            name: Path::new(relative).file_name().unwrap().to_os_string(),
            relative: relative.to_string(),
            bytes: Vec::new(),
        }
    }

    #[test]
    fn a_knowledge_file_is_shown_by_its_layer_and_its_path_within_it() {
        assert_eq!(
            shown_as(KnowledgeLayer::Repo, &file(None, "distilled/rule.md")),
            "repo/distilled/rule.md"
        );
        assert_eq!(
            shown_as(
                KnowledgeLayer::Org,
                &file(Some("acme/review"), "adr/0001.md")
            ),
            "org/acme/review/adr/0001.md"
        );
    }
}

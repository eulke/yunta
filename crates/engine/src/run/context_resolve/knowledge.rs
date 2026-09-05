//! `knowledge:` — the layered knowledge directories, most to least
//! local, with the org layer assembled from the installed packs.

use std::path::{Path, PathBuf};

use yunta_core::{Node, NodeId};

use super::error::ContextResolveError;
use crate::run::RunCtx;

/// Fixed precedence order: most general first, so a later layer
/// overwrites an earlier one by filename — `repo` wins over `user` wins
/// over `org` when they declare the same doc — never the order
/// `layers:` happens to name them in.
const KNOWLEDGE_PRECEDENCE: [yunta_core::KnowledgeLayer; 3] = [
    yunta_core::KnowledgeLayer::Org,
    yunta_core::KnowledgeLayer::User,
    yunta_core::KnowledgeLayer::Repo,
];

fn knowledge_dir(ctx: &RunCtx<'_>, layer: yunta_core::KnowledgeLayer) -> Option<PathBuf> {
    match layer {
        yunta_core::KnowledgeLayer::Repo => Some(ctx.worktree.join(".yunta").join("knowledge")),
        yunta_core::KnowledgeLayer::User => ctx
            .ambient
            .and_then(yunta_core::user_state_root)
            .map(|root| root.join("knowledge")),
        // Org is not one directory — it's the union of every installed
        // knowledge pack's declared contents; resolved by
        // `org_knowledge_files`, never through this single-dir path.
        yunta_core::KnowledgeLayer::Org => None,
    }
}

/// The org layer's files: every pack vendored under
/// `.yunta/packs/` whose `contents.knowledge` is non-empty contributes
/// each declared entry (a file, or a directory read recursively — the
/// same rule the other layers use). Returns `(filename, path)` pairs
/// after checking that no two *packs* ship the same filename — within
/// one pack the sorted listing's last occurrence wins, exactly the
/// silent-by-basename merge `repo`/`user` already have; across packs
/// there is no order to fall back on, so a collision is a typed error
/// naming both.
fn org_knowledge_files(
    ctx: &RunCtx<'_>,
    node: &NodeId,
    source_id: &str,
) -> Result<Vec<(std::ffi::OsString, PathBuf)>, ContextResolveError> {
    let mut by_name: std::collections::BTreeMap<std::ffi::OsString, (String, PathBuf)> =
        std::collections::BTreeMap::new();
    for publisher in crate::catalog::installed_publishers(ctx.worktree) {
        for (pack_dir, manifest) in
            crate::catalog::packs_for_publisher(ctx.worktree, &publisher).installed
        {
            let pack_label = format!("{}/{}", manifest.publisher, manifest.name);
            for declared in &manifest.contents.knowledge {
                let root = pack_dir.join(declared);
                let files = if root.is_file() {
                    vec![root]
                } else {
                    list_knowledge_files(&root, node, source_id)?
                };
                for path in files {
                    let Some(name) = path.file_name() else {
                        continue;
                    };
                    match by_name.get(name) {
                        Some((other_pack, _)) if *other_pack != pack_label => {
                            return Err(ContextResolveError::OrgKnowledgeCollision {
                                node: node.clone(),
                                source_id: source_id.to_string(),
                                file: name.to_string_lossy().into_owned(),
                                pack_a: other_pack.clone(),
                                pack_b: pack_label.clone(),
                            });
                        }
                        _ => {
                            by_name.insert(name.to_owned(), (pack_label.clone(), path));
                        }
                    }
                }
            }
        }
    }
    Ok(by_name
        .into_iter()
        .map(|(name, (_, path))| (name, path))
        .collect())
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
    // Recursive: the distilled subtree
    // (`distilled/<workflow>/<run>/…`) is part of the layer — the
    // distilled knowledge lives here too — so a flat listing would
    // silently hide exactly the knowledge the close deposited.
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

pub(super) async fn resolve_knowledge(
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

    // Precedence: the repo layer overrides the more general ones on
    // conflict — a later layer in KNOWLEDGE_PRECEDENCE overwrites an
    // earlier one by filename, so the same doc name in two layers
    // resolves to exactly one copy, never two. Org is the union of
    // installed knowledge packs, collision-checked among themselves
    // first.
    let mut by_name: std::collections::BTreeMap<std::ffi::OsString, PathBuf> =
        std::collections::BTreeMap::new();
    for layer in KNOWLEDGE_PRECEDENCE {
        if !requested.contains(&layer) {
            continue;
        }
        if layer == yunta_core::KnowledgeLayer::Org {
            for (name, path) in org_knowledge_files(ctx, &node.id, source_id)? {
                by_name.insert(name, path);
            }
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

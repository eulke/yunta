//! The files each knowledge layer holds, listed one way for the freeze and
//! for a checkout read. A file is named by its path within its root, so a
//! pack's file keeps the same name whether the run froze it or reads it live.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use yunta_core::KnowledgeLayer;

use super::FrozenError;
use crate::catalog::{self, DeclaredPath};

/// Where the repo's knowledge sits, relative to its checkout.
pub(crate) const REPO_KNOWLEDGE: &str = ".yunta/knowledge";

/// One file of a knowledge layer, as the layer's listing names it.
pub(crate) struct LayerFile {
    /// Where the file is, in the tree or the checkout it is listed from.
    pub(crate) path: PathBuf,
    /// The file's name, the key a layer's files merge by.
    pub(crate) name: OsString,
    /// The pack the file belongs to; `None` for the repo and user layers.
    pub(crate) pack: Option<String>,
    /// The file's path within its root, with `/` separators. A root that is
    /// itself a file names the file by its name.
    pub(crate) relative: String,
}

/// Every file `layer` holds: the repo's knowledge directory, the user's
/// directory when there is one, or the root each installed pack declares.
/// `org_root` places a declared root on disk, which is where the freeze
/// refuses a root that leaves its pack.
pub(crate) async fn layer_files(
    layer: KnowledgeLayer,
    repo: &Path,
    user: Option<&Path>,
    org_root: impl Fn(&DeclaredPath) -> Result<PathBuf, FrozenError>,
) -> Result<Vec<LayerFile>, FrozenError> {
    match layer {
        KnowledgeLayer::Repo => under_root(&repo.join(REPO_KNOWLEDGE), None).await,
        KnowledgeLayer::User => match user {
            Some(dir) => under_root(dir, None).await,
            None => Ok(Vec::new()),
        },
        KnowledgeLayer::Org => {
            let mut files = Vec::new();
            for declared in catalog::declared_knowledge(repo) {
                let root = org_root(&declared)?;
                files.extend(under_root(&root, Some(declared.pack.as_str())).await?);
            }
            Ok(files)
        }
    }
}

/// The files under one root, each named by its path within the root.
async fn under_root(root: &Path, pack: Option<&str>) -> Result<Vec<LayerFile>, FrozenError> {
    let mut files = Vec::new();
    for path in root_files(root).await? {
        let Some(name) = path.file_name().map(OsString::from) else {
            continue;
        };
        let relative = match path.strip_prefix(root) {
            Ok(inside) if !inside.as_os_str().is_empty() => inside.to_string_lossy().into_owned(),
            _ => name.to_string_lossy().into_owned(),
        };
        files.push(LayerFile {
            path,
            name,
            pack: pack.map(str::to_string),
            relative,
        });
    }
    Ok(files)
}

/// The files a declared knowledge root names: the file itself when the root
/// is one, else every file under the directory.
async fn root_files(root: &Path) -> Result<Vec<PathBuf>, FrozenError> {
    if tokio::fs::metadata(root)
        .await
        .is_ok_and(|meta| meta.is_file())
    {
        return Ok(vec![root.to_path_buf()]);
    }
    list_layer(root).await
}

/// Every regular file under `dir`, recursively, sorted; none when `dir` does
/// not exist. A symlink is neither a file nor a directory to this listing,
/// so it is not listed.
async fn list_layer(dir: &Path) -> Result<Vec<PathBuf>, FrozenError> {
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let io = |source: std::io::Error| FrozenError::Io {
            context: format!("list `{}`", current.display()),
            source,
        };
        let mut listing = tokio::fs::read_dir(&current).await.map_err(io)?;
        while let Some(entry) = listing.next_entry().await.map_err(io)? {
            let path = entry.path();
            match entry.file_type().await {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(kind) if kind.is_file() => entries.push(path),
                _ => {}
            }
        }
    }
    entries.sort();
    Ok(entries)
}

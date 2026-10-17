//! The files under a directory, listed one way for every frozen copy and for
//! the knowledge a freeze reads. What a listing does with a symlink, or with
//! any other entry that is neither a regular file nor a directory, is the
//! caller's policy.

use std::path::{Path, PathBuf};

use super::FrozenError;

/// What a listing does with an entry that is neither a regular file nor a
/// directory: a symlink, or a device, pipe or socket.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SymlinkPolicy<'a> {
    /// The entry is left out.
    Skip,
    /// A symlink to a regular file counts as that file, so a copy holds the
    /// bytes a session reads through the link. Any other entry is left out.
    FollowFiles,
    /// The entry is refused as a mismatch of `input`: a copy holds regular
    /// files and directories only.
    Refuse { input: &'a str },
}

/// Every regular file under `dir`, recursively, relative to `dir` and sorted.
/// A directory is descended, and a symlink to a directory is never followed,
/// so no link can loop.
pub(crate) async fn files(
    dir: &Path,
    policy: SymlinkPolicy<'_>,
) -> Result<Vec<PathBuf>, FrozenError> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let here = dir.join(&relative);
        let io = |source: std::io::Error| FrozenError::Io {
            context: format!("list `{}`", here.display()),
            source,
        };
        let mut listing = tokio::fs::read_dir(&here).await.map_err(io)?;
        while let Some(entry) = listing.next_entry().await.map_err(io)? {
            let path = relative.join(entry.file_name());
            let kind = match entry.file_type().await {
                Ok(kind) => kind,
                // A `Skip` listing leaves an entry it cannot type out; the
                // other policies fail the listing on it.
                Err(_) if matches!(policy, SymlinkPolicy::Skip) => continue,
                Err(source) => return Err(io(source)),
            };
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file()
                || lists_other(policy, kind.is_symlink(), &entry.path()).await?
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Whether an entry that is neither a regular file nor a directory is listed
/// under `policy`: a symlink to a file under `FollowFiles`, nothing under
/// `Skip`, and a refusal under `Refuse`.
async fn lists_other(
    policy: SymlinkPolicy<'_>,
    symlink: bool,
    path: &Path,
) -> Result<bool, FrozenError> {
    match policy {
        SymlinkPolicy::Skip => Ok(false),
        SymlinkPolicy::FollowFiles => Ok(symlink && is_file(path).await),
        SymlinkPolicy::Refuse { input } => Err(FrozenError::Mismatch {
            input: input.to_string(),
        }),
    }
}

/// Whether `path` is a file once its symlinks are followed.
async fn is_file(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .is_ok_and(|meta| meta.is_file())
}

//! The walk that tells what a build takes on disk. It reads the disk as
//! it goes, file by file, so it only ever runs on a blocking thread: the
//! walks of [`super::space`] hand it one.

// blocking: every read below happens on the thread `spawn_blocking` gives
// the walk, never on the executor.
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};

/// The bytes `roots` take on disk: each file by the blocks it uses, once
/// however many links name it, never past the device each root is on. A
/// root that is not there takes nothing, and an entry that vanishes during
/// the walk is skipped. `None` when something cannot be read for another
/// reason: a build nobody could measure is not taken for an empty one.
pub(super) fn on_disk(roots: &[PathBuf]) -> Option<u64> {
    match measure(roots) {
        Ok(total) => Some(total),
        Err(error) => {
            tracing::debug!(%error, "a checkout's build could not be measured");
            None
        }
    }
}

fn measure(roots: &[PathBuf]) -> io::Result<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut total = 0;
    for root in roots {
        let Some(top) = read_meta(root)? else {
            continue;
        };
        let device = device_of(&top);
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            let Some(meta) = read_meta(&path)? else {
                continue;
            };
            if device_of(&meta) != device || !seen.insert(identity(&meta, &path)) {
                continue;
            }
            total += blocks_of(&meta);
            if meta.is_dir() {
                if let Some(entries) = read_entries(&path)? {
                    pending.extend(entries);
                }
            }
        }
    }
    Ok(total)
}

/// What `path` is, read without following links; `None` when it is gone.
fn read_meta(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The paths directly inside `dir`; `None` when it is gone.
fn read_entries(dir: &Path) -> io::Result<Option<Vec<PathBuf>>> {
    let listing = match fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    listing
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<PathBuf>>>()
        .map(Some)
}

#[cfg(unix)]
fn device_of(meta: &Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::dev(meta)
}

#[cfg(not(unix))]
fn device_of(_meta: &Metadata) -> u64 {
    0
}

/// What tells one file from another: its inode where there is one.
#[cfg(unix)]
fn identity(meta: &Metadata, _path: &Path) -> (u64, u64, PathBuf) {
    use std::os::unix::fs::MetadataExt;
    (meta.dev(), meta.ino(), PathBuf::new())
}

#[cfg(not(unix))]
fn identity(_meta: &Metadata, path: &Path) -> (u64, u64, PathBuf) {
    (0, 0, path.to_path_buf())
}

#[cfg(unix)]
fn blocks_of(meta: &Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::blocks(meta) * 512
}

#[cfg(not(unix))]
fn blocks_of(meta: &Metadata) -> u64 {
    meta.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_named_twice_counts_once() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::write(&a, vec![7u8; 64 * 1024]).unwrap();
        std::fs::hard_link(&a, &b).unwrap();

        let once = on_disk(std::slice::from_ref(&a)).expect("a readable file is measured");

        assert!(once >= 64 * 1024, "{once}");
        assert_eq!(on_disk(&[a, b]), Some(once));
    }

    #[test]
    fn a_root_that_is_not_there_counts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(on_disk(&[dir.path().join("no-such-build")]), Some(0));
    }
}

//! The walk that tells what a build takes on disk. It reads the disk as
//! it goes, file by file, so it only ever runs on a blocking thread —
//! [`super::space::measure`] hands it there.

// blocking: every read below happens on the thread `spawn_blocking` gives
// the walk, never on the executor.
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};

/// The bytes `roots` take on disk: each file by the blocks it uses, once
/// however many links name it, never past the device each root is on.
pub(super) fn on_disk(roots: &[PathBuf]) -> u64 {
    let mut seen = std::collections::HashSet::new();
    let mut total = 0;
    for root in roots {
        let Ok(top) = fs::symlink_metadata(root) else {
            continue;
        };
        let device = device_of(&top);
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if device_of(&meta) != device || !seen.insert(identity(&meta, &path)) {
                continue;
            }
            total += blocks_of(&meta);
            if meta.is_dir() {
                if let Ok(entries) = fs::read_dir(&path) {
                    pending.extend(entries.flatten().map(|entry| entry.path()));
                }
            }
        }
    }
    total
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

        let once = on_disk(std::slice::from_ref(&a));

        assert!(once >= 64 * 1024, "{once}");
        assert_eq!(on_disk(&[a, b]), once);
    }
}

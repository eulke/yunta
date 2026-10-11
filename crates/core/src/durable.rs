//! Writes that leave a file whole or not at all: how this workspace
//! replaces or creates a file of engine state.
//!
//! The bytes go to a temporary file in the directory of the target, and
//! that file is renamed onto the target. The rename stays on one
//! filesystem, so a reader finds either the previous bytes or the new
//! ones, never a mix. `Synced` also flushes the bytes before the rename
//! and, on unix, the directory after it, so the new name survives a power
//! loss.
//!
//! Both calls block the thread they run on. A caller on an async task
//! marks each call with a `// blocking:` comment.

use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

/// How far a write goes before its call returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// The name changes whole: a reader finds the previous file or the new one.
    Atomic,
    /// As `Atomic`, with the bytes flushed before the rename and, on unix,
    /// the directory flushed after it.
    Synced,
}

/// Replaces `path` whole or not at all; a failed call leaves `path` and no staging file.
pub fn replace(path: &Path, bytes: &[u8], durability: Durability) -> io::Result<()> {
    let staged = stage(path, bytes, durability)?;
    staged.persist(path).map_err(|failed| failed.error)?;
    sync_directory(path, durability)
}

/// Creates `path` only when nothing is there; `Ok(false)` when it exists.
pub fn create_new(path: &Path, bytes: &[u8], durability: Durability) -> io::Result<bool> {
    let staged = stage(path, bytes, durability)?;
    match staged.persist_noclobber(path) {
        Ok(_) => {
            sync_directory(path, durability)?;
            Ok(true)
        }
        Err(failed) if failed.error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(failed) => Err(failed.error),
    }
}

/// The bytes in a temporary file beside `path`, flushed to disk first
/// when `durability` is `Synced`.
fn stage(path: &Path, bytes: &[u8], durability: Durability) -> io::Result<NamedTempFile> {
    let mut staged = NamedTempFile::new_in(directory_of(path))?;
    staged.write_all(bytes)?;
    if durability == Durability::Synced {
        staged.as_file().sync_all()?;
    }
    Ok(staged)
}

/// The directory `path` is named in. A bare file name's parent is the
/// empty path, which names the working directory.
fn directory_of(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Flushes the directory that names `path`, so the rename is on disk.
#[cfg(unix)]
fn sync_directory(path: &Path, durability: Durability) -> io::Result<()> {
    if durability == Durability::Atomic {
        return Ok(());
    }
    std::fs::File::open(directory_of(path))?.sync_all()
}

/// A directory cannot be flushed off unix: there the file flush is the whole
/// of what `Synced` makes durable.
#[cfg(not(unix))]
fn sync_directory(_path: &Path, _durability: Durability) -> io::Result<()> {
    Ok(())
}

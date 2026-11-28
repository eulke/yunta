//! How much a file system holds and how much of it is free — what keeps a
//! run from filling the disk it works on. A page that says what the disk has
//! free reads it through a [`Disk`] its caller hands it, the system's
//! ([`SystemDisk`]) unless a test fixes one; the checks that keep a run room
//! read the system's directly ([`space`]).

use std::path::Path;

/// What reads how much of a file system is free, for a page that says it: the
/// system's, or one a test fixes so a page reads the same on every machine.
/// Not what keeps a run room, which reads the system's ([`space`]).
pub trait Disk: Send + Sync {
    /// The space of the file system `path` is on.
    fn space(&self, path: &Path) -> std::io::Result<Space>;
}

/// The system's reading of a file system's space, [`space`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemDisk;

impl Disk for SystemDisk {
    fn space(&self, path: &Path) -> std::io::Result<Space> {
        space(path)
    }
}

/// A file system's size and what its user may still write there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub total: u64,
    /// What an unprivileged user may still write.
    pub available: u64,
}

/// The space of the file system `path` is on — `path` itself, or the
/// nearest directory above it that exists.
pub fn space(path: &Path) -> std::io::Result<Space> {
    let mut at = path;
    while !at.exists() {
        match at.parent() {
            Some(parent) => at = parent,
            None => break,
        }
    }
    let stat = rustix::fs::statvfs(at)?;
    let block = stat.f_frsize;
    Ok(Space {
        total: stat.f_blocks.saturating_mul(block),
        available: stat.f_bavail.saturating_mul(block),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_not_made_is_read_on_the_file_system_it_would_be_on() {
        let dir = tempfile::tempdir().unwrap();
        let made = space(dir.path()).unwrap();
        let unmade = space(&dir.path().join("not/made/yet")).unwrap();
        assert!(made.total > 0 && made.available <= made.total);
        assert_eq!(unmade.total, made.total);
    }
}

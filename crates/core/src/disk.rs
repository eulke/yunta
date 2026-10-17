//! How much a file system holds and how much of it is free — what keeps a
//! run from filling the disk it works on.

use std::path::Path;

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

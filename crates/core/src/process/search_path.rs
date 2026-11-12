//! See [`super`]. Where a program is found: the directories of the `PATH` an
//! environment hands what it starts, searched in order as the system's own
//! search takes them. Whatever starts a program by its name finds it here, on
//! the environment the shell captured: an adapter's CLI, for a session and
//! for the probe that vouches for it, `git`, the shell a node's command runs
//! in, a program a pack requires. None of them reads the process's own
//! `PATH`, so a probe finds the CLI its session runs.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The directories a program named without one is looked up in, in the order
/// they are searched. Not the `PATH` variable's text, which
/// [`SearchPath::of_var`] reads and [`SearchPath::joined`] writes.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SearchPath(Vec<PathBuf>);

impl SearchPath {
    /// A search through `dirs`, in their order.
    pub const fn new(dirs: Vec<PathBuf>) -> Self {
        SearchPath(dirs)
    }

    /// The directories a `PATH` variable holding `value` names.
    pub fn of_var(value: &OsStr) -> Self {
        SearchPath(std::env::split_paths(value).collect())
    }

    /// The directories, in the order they are searched.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.0
    }

    /// Whether it names no directory.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value a `PATH` variable holds for this search: `None` where it
    /// names no directory, or one a `PATH` cannot hold.
    pub fn joined(&self) -> Option<OsString> {
        match self.0.is_empty() {
            true => None,
            false => std::env::join_paths(&self.0).ok(),
        }
    }

    /// Where `program` runs from, for a process that searches here: a path
    /// names itself, where it names a file; a bare name is the first file by
    /// that name in these directories that a process may execute. `None`
    /// where neither is there.
    pub fn find(&self, program: impl AsRef<Path>) -> Option<PathBuf> {
        let program = program.as_ref();
        if program.components().count() > 1 {
            return program.is_file().then(|| program.to_path_buf());
        }
        self.0
            .iter()
            .map(|dir| dir.join(program))
            .find(|candidate| executable(candidate))
    }
}

/// Whether `path` is a file a process may execute: one with an execute bit
/// set. The system's own search passes over any other.
#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Without execute bits, every file a search finds is one it would run.
#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests;

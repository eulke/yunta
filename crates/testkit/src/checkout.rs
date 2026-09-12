//! `Checkout` — the world a test that drives the compiled binary runs in.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::repo::{git, init_repo, write};

/// A committed git checkout and the state root a run against it keeps
/// its runs in — everything `yunta` needs on disk before it is spawned.
///
/// The sibling of [`Bench`], which drives the engine in process against
/// an open event store. This one drives the binary, which opens its own:
/// a test builds the tree, spawns the command, and reads back what a
/// person would have seen.
///
/// Built up and then committed, because every run starts from a clean
/// tree and a workflow that is not committed is not in one.
///
/// [`Bench`]: crate::Bench
pub struct Checkout {
    _root: Option<TempDir>,
    /// The checkout a run's nodes execute against.
    pub repo: PathBuf,
    /// The state root run directories are created under.
    pub home: PathBuf,
}

impl Checkout {
    /// An empty checkout under a temporary directory it owns.
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let mut checkout = Self::under(root.path());
        checkout._root = Some(root);
        checkout
    }

    /// An empty checkout under a directory the caller owns — for a test
    /// that keeps the tree alive past the `Checkout` itself, or puts
    /// two side by side.
    ///
    /// No config is written: what a run's isolation should be is the
    /// thing a test says out loud, not something it inherits.
    pub fn under(root: &Path) -> Self {
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).expect("create the repo directory");
        init_repo(&repo);
        Checkout {
            _root: None,
            repo,
            home: root.join("state"),
        }
    }

    /// Declares `isolation: none`, which keeps every node's work in this
    /// tree — what lets a test read back what a run produced. A run that
    /// composes children leaves this off: a child is a run of its own
    /// and needs the default isolation.
    pub fn working_in_place(self) -> Self {
        self.config("defaults:\n  isolation: none\n")
    }

    /// Writes one file into the checkout, creating parent directories.
    pub fn file(self, path: &str, contents: &str) -> Self {
        write(&self.repo.join(path), contents);
        self
    }

    /// Writes a workflow as `<name>.yaml`.
    pub fn workflow(self, name: &str, contents: &str) -> Self {
        self.file(&format!("{name}.yaml"), contents)
    }

    /// Replaces `.yunta/config.yaml` — for a test whose runners, forge
    /// or limits are the thing under test.
    pub fn config(self, contents: &str) -> Self {
        self.file(".yunta/config.yaml", contents)
    }

    /// Commits everything written so far, leaving the tree clean.
    pub fn committed(self) -> Self {
        git(&self.repo, &["add", "."]);
        git(&self.repo, &["commit", "-q", "-m", "fixtures"]);
        self
    }
}

impl Default for Checkout {
    fn default() -> Self {
        Self::new()
    }
}

//! `Checkout` — the world a test that drives the compiled binary runs in.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
    /// Whether the binary is told where its state root is. A run that
    /// is not told puts it under the home it was given, which is the
    /// thing one test measures.
    told_its_home: bool,
    /// The org layer the binary reads. An org layer is a ceiling the
    /// layers under it can only narrow, so it is empty unless a test
    /// hands one over — a test that inherited the machine's would
    /// measure the machine.
    org_config: String,
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
            told_its_home: true,
            org_config: String::new(),
        }
    }

    /// Leaves the binary without a `YUNTA_HOME`, so where its state root
    /// lands is what the run decides from the home it was given — what
    /// a test about that default measures.
    pub fn without_yunta_home(mut self) -> Self {
        self.told_its_home = false;
        self
    }

    /// Hands the binary an org layer, the ceiling every lower layer can
    /// only narrow — for a test about what that ceiling refuses.
    pub fn with_org_config(mut self, yaml: &str) -> Self {
        self.org_config = yaml.to_string();
        self
    }

    /// The command that runs the compiled binary at `bin` in this
    /// checkout: hermetic, and then whatever this checkout says about
    /// its home and its org layer. Use the
    /// [`yunta_at!`](crate::yunta_at) macro rather than calling this
    /// directly — it fills in the binary path from the calling crate's
    /// `CARGO_BIN_EXE_yunta`.
    pub fn command(&self, bin: &Path) -> Command {
        let mut command = Command::new(bin);
        crate::bin::hermetic(&mut command, &self.repo, &self.home);
        if !self.told_its_home {
            command.env_remove("YUNTA_HOME");
        }
        if !self.org_config.is_empty() {
            write(&self.home.join("org.yaml"), &self.org_config);
        }
        command
    }

    /// Runs the binary and answers with what it wrote, stdin closed.
    pub fn run(&self, bin: &Path, args: &[&str]) -> Output {
        self.command(bin)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("failed to run the yunta binary")
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

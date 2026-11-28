//! The pack a setup installs, as the Yunta repository holds it, in a local repository a
//! test reaches in the Yunta repository's place: `packs/fragua` copied from this checkout
//! and committed under the binary's version tag ([`ReferencePack`]), alone or on a commit
//! after another, which a clone of the tag alone leaves behind; and the environment that
//! points git at it for the Yunta repository's URL, by its path ([`ReferencePack::reached`])
//! or by its `file://` URL, which git's own transport reads as it reads a remote, or at a
//! directory no repository stands in ([`out_of_reach`]).
//!
//! Git's own `url.<base>.insteadOf` does the pointing, handed to git through the
//! environment (`GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0`, `GIT_CONFIG_VALUE_0`): a
//! workspace's `subprocess_vars` in process, a process's environment at a terminal. No
//! test asks the network for the Yunta repository, and the product holds no hook for it.
//!
//! Every commit here is written by one author at one instant, so it is the same commit
//! wherever it is made, and what a lock or a run's manifest records of it is too.

use std::path::{Path, PathBuf};

/// The Yunta repository's URL, as the package definition names it.
pub const YUNTA_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The tag the Yunta repository marks this binary's version with.
pub const VERSION_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Where the reference pack stands in the Yunta repository.
pub const FRAGUA: &str = "packs/fragua";

/// The instant every commit here is dated by, as a commit records one: the day the
/// design's repository last moved before its runs.
const COMMITTED_AT: &str = "1767139200 +0000";

/// Who makes every commit here.
const AUTHOR: (&str, &str) = ("Test", "test@example.com");

/// The Yunta repository as a test reaches it: `packs/fragua`, committed once and tagged
/// with the binary's version. Not the pack installed: the repository a setup fetches it
/// from.
pub struct ReferencePack {
    repository: PathBuf,
}

impl ReferencePack {
    /// The Yunta repository made at `at`, which does not stand yet: this checkout's
    /// `packs/fragua` in one commit, tagged [`VERSION_TAG`].
    pub fn made(at: &Path) -> ReferencePack {
        copied(&checked_out(), &at.join(FRAGUA));
        committed(at);
        commit_git(at, &["tag", VERSION_TAG]);
        ReferencePack {
            repository: at.to_path_buf(),
        }
    }

    /// The Yunta repository made at `at` as [`ReferencePack::made`] makes it, on a commit
    /// of its own before the one [`VERSION_TAG`] names: a clone of the tag alone holds one
    /// commit of the two, a whole clone both.
    pub fn made_with_history(at: &Path) -> ReferencePack {
        std::fs::create_dir_all(at).expect("the repository's directory");
        commit_git(at, &["init", "-q", "-b", crate::INITIAL_BRANCH]);
        std::fs::write(at.join("README.md"), "the Yunta repository\n").expect("a readme");
        commit_git(at, &["add", "-A"]);
        commit_git(at, &["commit", "-q", "-m", "before fragua"]);
        copied(&checked_out(), &at.join(FRAGUA));
        commit_git(at, &["add", "-A"]);
        commit_git(at, &["commit", "-q", "-m", "fragua"]);
        commit_git(at, &["tag", VERSION_TAG]);
        ReferencePack {
            repository: at.to_path_buf(),
        }
    }

    /// Where the repository stands.
    pub fn repository(&self) -> &Path {
        &self.repository
    }

    /// The pack's own directory in the repository's working tree.
    pub fn pack(&self) -> PathBuf {
        self.repository.join(FRAGUA)
    }

    /// The commit [`VERSION_TAG`] names.
    pub fn commit(&self) -> String {
        commit_of(&self.repository)
    }

    /// The pack alone at the root of a repository of its own made at `at`, committed as
    /// the reference pack is: a source `yunta pack add` reads a pack from the root of.
    /// Answers where it stands.
    pub fn at_a_root(&self, at: &Path) -> PathBuf {
        copied(&self.pack(), at);
        committed(at);
        at.to_path_buf()
    }

    /// The environment that points git at this repository for [`YUNTA_REPOSITORY`].
    pub fn reached(&self) -> Vec<(String, String)> {
        pointed_at(&self.repository.display().to_string())
    }

    /// The environment that points git at this repository's `file://` URL for
    /// [`YUNTA_REPOSITORY`]: reached through git's own transport, as a remote is, so a
    /// clone at depth one takes what it asks, where a local path is copied whole.
    pub fn reached_through_its_url(&self) -> Vec<(String, String)> {
        pointed_at(&format!("file://{}", self.repository.display()))
    }
}

/// The environment that points git at `nowhere`, a directory no repository stands in,
/// for [`YUNTA_REPOSITORY`]: the pack a setup installs, out of reach, and no network
/// asked.
pub fn out_of_reach(nowhere: &Path) -> Vec<(String, String)> {
    pointed_at(&nowhere.display().to_string())
}

/// The environment that has git read `local`, a path or a URL, wherever it is asked for
/// [`YUNTA_REPOSITORY`].
fn pointed_at(local: &str) -> Vec<(String, String)> {
    vec![
        ("GIT_CONFIG_COUNT".to_string(), "1".to_string()),
        (
            "GIT_CONFIG_KEY_0".to_string(),
            format!("url.{local}.insteadOf"),
        ),
        (
            "GIT_CONFIG_VALUE_0".to_string(),
            YUNTA_REPOSITORY.to_string(),
        ),
    ]
}

/// Where this checkout holds the reference pack.
fn checked_out() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(FRAGUA)
}

/// Every file under `from` copied under `to`, directories made as they come.
fn copied(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory of the pack");
    for entry in std::fs::read_dir(from).expect("a directory of the pack, read") {
        let entry = entry.expect("an entry of the pack");
        let path = entry.path();
        let into = to.join(entry.file_name());
        match path.is_dir() {
            true => copied(&path, &into),
            false => {
                std::fs::copy(&path, &into).expect("a file of the pack, copied");
            }
        }
    }
}

/// A repository started at `repo`, everything under it committed once.
fn committed(repo: &Path) {
    commit_git(repo, &["init", "-q", "-b", crate::INITIAL_BRANCH]);
    commit_git(repo, &["add", "-A"]);
    commit_git(repo, &["commit", "-q", "-m", "fragua"]);
}

/// The commit `repo` stands at.
fn commit_of(repo: &Path) -> String {
    crate::git_output(repo, &["rev-parse", "HEAD"])
}

/// Runs `git` in `repo` as [`AUTHOR`] at [`COMMITTED_AT`], reading none of the
/// machine's configuration.
fn commit_git(repo: &Path, args: &[&str]) {
    let (name, email) = AUTHOR;
    let dated = format!("@{COMMITTED_AT}");
    let signed = [
        ("GIT_AUTHOR_NAME", name),
        ("GIT_AUTHOR_EMAIL", email),
        ("GIT_AUTHOR_DATE", &dated),
        ("GIT_COMMITTER_NAME", name),
        ("GIT_COMMITTER_EMAIL", email),
        ("GIT_COMMITTER_DATE", &dated),
    ]
    .map(|(key, value)| (key.to_string(), value.to_string()));
    let output = crate::git_with(repo, args, &signed);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

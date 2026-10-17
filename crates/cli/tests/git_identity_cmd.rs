//! A run commits every node's work, so where git cannot name who commits,
//! `yunta run` refuses before anything exists and `yunta doctor` says why.

use std::path::PathBuf;

use yunta_testkit::{checked, git, init_repo, stderr, stdout, write, yunta_in};

/// A node whose work is a file: closing it makes the run's first commit.
const ONE_NODE: &str =
    "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"echo done > done.txt\"\n";

/// A repository whose git has no name to commit under and is told not
/// to guess one from the host.
fn a_repo_without_identity() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    git(&repo, &["config", "--unset", "user.email"]);
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "user.useConfigOnly", "true"]);
    write(&repo.join("wf.yaml"), ONE_NODE);
    let home = root.path().join("state");
    (root, repo, home)
}

#[test]
fn a_run_is_refused_where_git_has_no_identity() {
    let (_root, repo, home) = a_repo_without_identity();

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);

    assert!(!run.status.success(), "stdout: {}", stdout(&run));
    let said = stderr(&run);
    assert!(
        said.contains("a run commits every node's work, and git cannot name who commits here")
            && said.contains("git config --global user.email")
            && said.contains("auto-detection is disabled"),
        "{said}"
    );
    let listed = yunta_in!(&repo, &home, &["list", "--runs"]);
    assert!(
        stdout(&listed).starts_with("no runs"),
        "{}",
        stdout(&listed)
    );
}

#[test]
fn doctor_says_who_git_commits_as_and_when_it_cannot() {
    let (_root, repo, home) = a_repo_without_identity();

    let without = yunta_in!(&repo, &home, &["doctor"]);
    assert!(
        checked(&stdout(&without), "git")
            .is_some_and(|said| said.starts_with("cannot name who commits here")),
        "{}",
        stdout(&without)
    );
    assert!(!without.status.success());

    git(&repo, &["config", "user.name", "Ada"]);
    git(&repo, &["config", "user.email", "ada@example.com"]);
    let with = yunta_in!(&repo, &home, &["doctor"]);
    assert_eq!(
        checked(&stdout(&with), "git").as_deref(),
        Some("commits as Ada <ada@example.com>"),
        "{}",
        stdout(&with)
    );
}

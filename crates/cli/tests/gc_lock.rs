//! `yunta gc` takes the worktree-mutation lock around the worktree calls it
//! makes: a worktree it forgets is pruned while the lock is held.

use yunta_testkit::stubs::{GIT_LOCK_PROBE, GIT_LOG};
use yunta_testkit::{git, init_repo, stderr, write, yunta_in};

const ONE_NODE: &str = "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n";

/// A repository whose one finished run is past its zero-day retention, and
/// a worktree git still lists after its checkout went: what `gc` collects
/// when it forgets the units of a run it removes. Only a run `gc` removes in
/// its pass has its units forgotten, which is why the retention is zero and
/// the run finishes. Returns the directory the repository and the git stub
/// live in, the state root, and the log the stub appends to.
fn abandoned_worktree_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    write(&repo.join("wf.yaml"), ONE_NODE);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let abandoned = root.path().join("abandoned");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            abandoned.to_str().unwrap(),
        ],
    );
    std::fs::remove_dir_all(&abandoned).unwrap();
    let log = root.path().join("git.log");
    (root, home, log)
}

#[test]
fn gc_removes_an_abandoned_worktree_under_the_mutation_lock() {
    let (root, home, log) = abandoned_worktree_fixture();
    let dir = root.path();
    let mut env = yunta_testkit::stubs::git(dir);
    env.push((
        GIT_LOCK_PROBE.into(),
        dir.join("repo/.git/yunta-worktree.lock")
            .display()
            .to_string(),
    ));
    env.push((GIT_LOG.into(), log.display().to_string()));

    yunta_in!(&dir.join("repo"), &home, &["gc"], &env);

    assert!(std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .any(|l| l.contains("worktree prune") && l.ends_with("locked")));
}

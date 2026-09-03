//! `yunta gc`: reclaims a terminal run's `run.dir`/worktree pair once
//! `storage.retention_days` has passed, in a fixed death order (files
//! first, event-log rows only on a later pass). A run is found and
//! removed by the paths *frozen in its manifest*, so a `paths.*` change
//! after the run was created never loses it; a removal that fails is
//! warned and left uncounted, never reported as reclaimed.

use yunta_testkit::{init_repo, run_id_from, stderr, stdout, write, yunta_in};

const ONE_NODE: &str = "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n";

#[test]
fn gc_does_nothing_when_retention_days_is_not_configured() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let gc = yunta_in!(&repo, &home, &["gc"]);
    assert!(gc.status.success());
    assert!(stdout(&gc).contains("retention_days"));
}

#[test]
fn gc_reclaims_a_finished_run_past_its_retention_window() {
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
    assert!(run.status.success());
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);
    assert!(run_dir.exists());

    let gc = yunta_in!(&repo, &home, &["gc"]);
    assert!(gc.status.success(), "stderr: {}", stderr(&gc));
    assert!(stdout(&gc).contains("reclaimed"), "got: {}", stdout(&gc));
    assert!(!run_dir.exists(), "run.dir should have been removed");
}

#[test]
fn gc_dry_run_reports_without_removing_anything() {
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
    assert!(run.status.success());
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);

    let gc = yunta_in!(&repo, &home, &["gc", "--dry-run"]);
    assert!(gc.status.success());
    assert!(
        stdout(&gc).contains("would be reclaimed"),
        "got: {}",
        stdout(&gc)
    );
    assert!(run_dir.exists(), "dry-run must never remove anything");
}

#[test]
fn gc_reclaims_files_first_and_purges_rows_only_on_a_later_pass() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: short\nnodes:\n  - id: fine\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);
    assert!(run_dir.exists());

    // Pass 1: files die, rows survive — the DB is never first to go.
    let first = yunta_in!(&repo, &home, &["gc"]);
    assert!(first.status.success());
    assert!(!run_dir.exists(), "run.dir reclaimed: {}", stdout(&first));
    // The rows survive pass 1 — `verify` (which needs only the DB)
    // still walks the chain.
    let verify = yunta_in!(&repo, &home, &["verify", &run_id]);
    assert!(
        verify.status.success() && stdout(&verify).contains("intact"),
        "rows still readable after pass 1: {}",
        stderr(&verify)
    );

    // Pass 2: the dir is gone, so the rows go now.
    let second = yunta_in!(&repo, &home, &["gc"]);
    assert!(second.status.success());
    assert!(
        stdout(&second).contains("purged"),
        "got: {}",
        stdout(&second)
    );

    // A purged run reads back as unknown — never corrupt state.
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(!status.status.success());
    let verify = yunta_in!(&repo, &home, &["verify", &run_id]);
    assert!(!verify.status.success());
    assert!(
        stderr(&verify).contains("no events"),
        "got: {}",
        stderr(&verify)
    );
}

#[test]
fn gc_finds_runs_by_frozen_paths_after_a_config_change() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // The run is created under the default state paths.
    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    write(&repo.join("wf.yaml"), ONE_NODE);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);
    let worktree = home.join("worktrees").join(&run_id);
    assert!(run_dir.exists() && worktree.exists());

    // The config's paths now point elsewhere — the run stays frozen where
    // it was created, and gc must follow the manifest, not this config.
    let relocated_runs = home.join("relocated-runs");
    let relocated_worktrees = home.join("relocated-worktrees");
    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            "storage:\n  retention_days: 0\npaths:\n  runs: {}\n  worktrees: {}\n",
            relocated_runs.display(),
            relocated_worktrees.display()
        ),
    );

    let gc = yunta_in!(&repo, &home, &["gc"]);
    assert!(gc.status.success(), "stderr: {}", stderr(&gc));
    assert!(stdout(&gc).contains("reclaimed"), "got: {}", stdout(&gc));

    // The run's real (frozen) run.dir and worktree are gone; the current
    // config's roots never were this run's home.
    assert!(
        !run_dir.exists(),
        "the frozen run.dir must be reclaimed: {}",
        stdout(&gc)
    );
    assert!(
        !worktree.exists(),
        "the frozen worktree must be reclaimed, not the current config's: {}",
        stdout(&gc)
    );
    assert!(!relocated_runs.join(&run_id).exists());
    assert!(!relocated_worktrees.join(&run_id).exists());
}

#[test]
fn failed_removal_is_not_counted() {
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

    // Two finished runs, both past the zero-day retention window.
    let clean = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(clean.status.success());
    let clean_id = run_id_from(&clean);
    let stuck = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(stuck.status.success());
    let stuck_id = run_id_from(&stuck);

    // One run's worktree cannot be removed as a directory: a regular file
    // in its place makes `remove_dir_all` fail with ENOTDIR for any user,
    // root included — a removal that can never succeed.
    let stuck_worktree = home.join("worktrees").join(&stuck_id);
    std::fs::remove_dir_all(&stuck_worktree).unwrap();
    std::fs::write(&stuck_worktree, b"not a directory").unwrap();

    let gc = yunta_in!(&repo, &home, &["gc"]);
    assert!(
        gc.status.success(),
        "a failed removal warns, never aborts: {}",
        stderr(&gc)
    );
    // Only the run gc fully reclaimed is counted; the stuck one is not,
    // even though it too is terminal and past retention.
    assert!(
        stdout(&gc).contains("1 run(s) reclaimed"),
        "exactly the clean run is counted: {}",
        stdout(&gc)
    );
    // The failure is surfaced, never swallowed.
    assert!(
        stderr(&gc).contains("warning"),
        "the stuck removal is warned: {}",
        stderr(&gc)
    );
    // The clean run is gone; the un-removable worktree is still there.
    assert!(!home.join("runs").join(&clean_id).exists());
    assert!(stuck_worktree.exists(), "the removal genuinely failed");
}

#[test]
fn gc_leaves_a_paused_run_however_old_it_is() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    // A failing node with no `on_failure` pauses the run: it never reaches a
    // terminal state, so gc must leave its footprint alone however far past
    // the retention window it is — only terminal runs are reclaimed.
    write(
        &repo.join("wf.yaml"),
        "name: stuck\nnodes:\n  - id: broken\n    kind: bash\n    run: \"false\"\n",
    );
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);
    assert!(
        run_dir.exists(),
        "the paused run was created: {}",
        stderr(&run)
    );

    let gc = yunta_in!(&repo, &home, &["gc"]);
    assert!(gc.status.success(), "stderr: {}", stderr(&gc));
    assert!(
        stdout(&gc).contains("nothing to reclaim"),
        "a non-terminal run offers nothing to reclaim: {}",
        stdout(&gc)
    );
    assert!(
        run_dir.exists(),
        "a paused run's footprint must survive gc, however old"
    );
}

//! `yunta gc`: reclaims a terminal run's `run.dir`/worktree pair once
//! `storage.retention_days` has passed, in a fixed death order (files
//! first, event-log rows only on a later pass). A run is found and
//! removed by the paths *frozen in its manifest*, so a `paths.*` change
//! after the run was created never loses it; a removal that fails is
//! warned and left uncounted, never reported as reclaimed.

use yunta_testkit::{git, git_output, init_repo, run_id_from, stderr, stdout, write, yunta_in};

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

/// What git held of a collected run goes with its files: a branch one of
/// its units left work on that never landed. The project's checkouts stay,
/// on no branch, for the runs to come.
#[test]
fn gc_takes_a_collected_run_s_unit_branches() {
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
        "name: scoped\nnodes:\n  - id: only\n    kind: bash\n    run: \"echo a > a.txt\"\n    scope: [\"a.txt\"]\n",
    );
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let left = format!("yunta/unit/{}/task/T1/1", run_id_from(&run));
    git(&repo, &["branch", &left]);
    let units = || {
        git_output(
            &repo,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads/yunta/unit/",
            ],
        )
    };
    assert!(!units().trim().is_empty(), "the run left a unit's work");

    let gc = yunta_in!(&repo, &home, &["gc"]);

    assert!(gc.status.success(), "stderr: {}", stderr(&gc));
    assert_eq!(units().trim(), "");
    let listing = git_output(&repo, &["worktree", "list", "--porcelain"]);
    assert!(!listing.contains("refs/heads/yunta/unit/"), "{listing}");
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
    let checkouts = yunta_testkit::pool_checkouts(&home);
    assert!(run_dir.exists() && checkouts.len() == 1, "{checkouts:?}");

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

    // The run's real (frozen) run.dir is gone; the current config's roots
    // never were this run's home. Its checkout went back to the project.
    let said = stdout(&gc);
    assert!(!run_dir.exists(), "the frozen run.dir is reclaimed: {said}");
    assert!(checkouts[0].exists(), "and its checkout given back: {said}");
    assert!(!relocated_runs.join(&run_id).exists());
    assert!(!relocated_worktrees.join(&run_id).exists());
}

#[test]
fn failed_removal_is_not_counted() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = keeping_nothing(root.path());

    // Two finished runs, both past the zero-day retention window.
    let [clean_id, stuck_id] = ["clean", "stuck"].map(|_| {
        let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
        assert!(run.status.success());
        run_id_from(&run)
    });

    // One run's directory cannot be removed: a directory in it that lets
    // nobody take what is inside. Root takes it anyway, so a root test has
    // nothing to show.
    if running_as_root() {
        return;
    }
    let stuck_dir = locked_in(&home.join("runs").join(&stuck_id));

    let gc = yunta_in!(&repo, &home, &["gc"]);
    let (said, warned) = (stdout(&gc), stderr(&gc));
    assert!(
        gc.status.success(),
        "a failed removal warns, never aborts: {warned}"
    );
    // Only the run gc fully reclaimed is counted; the stuck one is not,
    // even though it too is terminal and past retention.
    assert!(
        said.contains("1 run reclaimed"),
        "only the clean run counts: {said}"
    );
    // The failure is surfaced, never swallowed.
    assert!(
        warned.contains("warning"),
        "the stuck removal is warned: {warned}"
    );
    // The clean run is gone; the un-removable directory is still there.
    assert!(!home.join("runs").join(&clean_id).exists());
    assert!(
        stuck_dir.join("kept").exists(),
        "the removal genuinely failed"
    );
    set_mode(&stuck_dir, 0o700);
}

/// A repository under `root` whose config keeps nothing past a day, with
/// its one-node workflow, and the state root beside it.
fn keeping_nothing(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let config = "storage:\n  retention_days: 0\n";
    write(&repo.join(".yunta/config.yaml"), config);
    write(&repo.join("wf.yaml"), ONE_NODE);
    (repo, root.join("state"))
}

/// A directory under `run_dir` that lets nobody take what is inside it.
fn locked_in(run_dir: &std::path::Path) -> std::path::PathBuf {
    let locked = run_dir.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("kept"), b"kept").unwrap();
    set_mode(&locked, 0o500);
    locked
}

fn set_mode(dir: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn running_as_root() -> bool {
    let id = std::process::Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&id.stdout).trim() == "0"
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

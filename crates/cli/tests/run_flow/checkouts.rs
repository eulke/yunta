//! Where a run works: a worktree of its own or this checkout, and what stays when it ends.

use super::*;

#[test]
fn worktree_isolation_is_the_default_and_agent_edits_never_touch_the_original_checkout() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: touches-a-file
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );

    // The default (`worktree`) isolation must never let the agent's edit
    // land in the checkout the user is looking at.
    assert!(!repo.join("made.txt").exists());
    // It lives in a checkout of the project's pool under the state root.
    let made_somewhere = yunta_testkit::pool_checkouts(&home)
        .iter()
        .any(|checkout| checkout.join("made.txt").exists());
    assert!(
        made_somewhere,
        "expected made.txt in a checkout of the pool under {}",
        home.display()
    );
}

/// Two runs one after the other work in the same checkout of the project's
/// pool, each on a branch of its own: the second finds what the first
/// built, and each branch holds only its own run's work.
#[test]
fn two_runs_on_the_same_repo_share_one_warm_checkout_on_branches_of_their_own() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: touches-a-file
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
"#,
    );

    let run1 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run1.status.success(), "run1: {}", stdout(&run1));
    let run2 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run2.status.success(), "run2: {}", stdout(&run2));

    assert_eq!(yunta_testkit::pool_checkouts(&home).len(), 1);
    for run in [&run1, &run2] {
        let branch = format!("yunta/run/{}:made.txt", run_id_from(run));
        let made = git_output(&repo, &["show", &branch]);
        assert_eq!(made.trim(), "made", "{branch}");
    }
}

#[test]
fn isolation_none_refuses_a_dirty_tree_before_creating_any_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: needs-clean-tree
nodes:
  - id: touch
    kind: bash
    run: "true"
"#,
    );
    // Dirty the tree.
    write(&repo.join("uncommitted.txt"), "dirty");

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    assert!(
        !home.join("runs").exists(),
        "no run must be created when isolation refuses the dirty tree"
    );
}

#[test]
fn isolation_none_operates_directly_on_the_checkout_and_releases_its_lock_when_finished() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: no-op
nodes:
  - id: noop
    kind: bash
    run: "true"
"#,
    );
    // Isolation `none` requires a clean tree — commit the fixtures
    // themselves so only the workflow's own effects could dirty it.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run1 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run1.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run1),
        String::from_utf8_lossy(&run1.stderr)
    );
    // Finishing must release the lock so a second run on the same
    // (still clean) checkout can proceed.
    let run2 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run2.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run2),
        String::from_utf8_lossy(&run2.stderr)
    );
    // `none` never creates a dedicated worktree.
    assert!(!home.join("worktrees").exists());
}

#[test]
fn on_finish_cleanup_removes_the_worktree_and_keeps_the_run_dir() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: tidy
nodes:
  - id: fine
    kind: bash
    run: "true"
on_finish:
  - cleanup: worktree
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_id = run_id_from(&run);

    assert!(
        !home.join("worktrees").join(&run_id).exists(),
        "the declared cleanup must remove the run's worktree"
    );
    assert!(
        home.join("runs")
            .join(&run_id)
            .join("manifest.yaml")
            .exists(),
        "run.dir stays — cleanup is about the checkout, never the audit trail"
    );
    // The run branch pointed at the base commit (nothing was committed),
    // so `git branch -d` agreed to delete it.
    let branches = git_output(&repo, &["branch", "--list", &format!("yunta/run/{run_id}")]);
    assert!(branches.is_empty(), "a merged run branch is deleted");
}

#[test]
fn distill_under_isolation_none_leaves_uncommitted_files_and_the_next_run_refuses() {
    // The engine never commits the user's own branch: distilled files
    // stay visible and uncommitted, and the next `none` run refuses the
    // dirty tree until a human commits or discards — deliberate
    // friction, not a bug.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: distill-none
nodes:
  - id: plan
    kind: bash
    run: "echo durable > {{node.artifacts}}/plan.md"
    artifacts:
      produces: [plan.md]
on_finish:
  - distill: [{ node: plan, name: plan.md }]
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let distilled = repo.join(".yunta/knowledge/distilled/distill-none");
    assert!(distilled.exists(), "the files land in the user's checkout");
    let status = git_output(&repo, &["status", "--porcelain"]);
    assert!(
        status.contains(".yunta/knowledge"),
        "uncommitted — the engine never commits the user's branch"
    );

    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        !second.status.success(),
        "the dirty tree must refuse the next `none` run"
    );
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("clean tree"),
        "got: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}

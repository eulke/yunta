use super::*;

/// A `worktree add` killed half way leaves its checkout marked as being
/// added, which a plain `--force` refuses to remove: the undo takes it
/// away, and git's record of it with it.
#[tokio::test]
async fn the_undo_of_a_cut_add_takes_away_a_checkout_git_still_marks_as_being_added() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let checkout = root.path().join("slot-1");
    let shown = checkout.display().to_string();
    yunta_testkit::git(&repo, &["worktree", "add", "--detach", &shown]);
    yunta_testkit::git(
        &repo,
        &["worktree", "lock", "--reason", "initializing", &shown],
    );
    // Killed before it wrote where the checkout stands.
    let pointer = std::fs::read_to_string(checkout.join(".git")).unwrap();
    let admin = PathBuf::from(pointer.trim().strip_prefix("gitdir: ").unwrap());
    std::fs::rename(admin.join("HEAD"), admin.join("HEAD.lock")).unwrap();
    let token = CancellationToken::new();
    let clock = yunta_testkit_core::FixedClock;
    let supervision = Supervision::outside_any_run(&token, &clock);
    let common = git::common_dir(&repo, supervision).await.unwrap();
    let held = git::mutation_lock(&common, supervision).await.unwrap();

    undo_half_added(&held, &repo, &checkout, None, supervision).await;

    assert!(!checkout.exists(), "the checkout is gone");
    let listed = git::read(&repo, &["worktree", "list", "--porcelain"], supervision)
        .await
        .unwrap();
    assert!(!listed.contains("slot-1"), "git still records it: {listed}");
}

#[test]
fn a_common_dir_without_a_parent_names_no_main_repo() {
    let err = main_repo_of(Path::new("/")).unwrap_err();
    assert!(
        matches!(err, WorktreeError::NoMainRepo { .. }),
        "got: {err:?}"
    );
    assert_eq!(
        main_repo_of(Path::new("/srv/repo/.git")).unwrap(),
        PathBuf::from("/srv/repo")
    );
}

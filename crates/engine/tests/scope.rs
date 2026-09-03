use yunta_engine::{scope_check, ScopeCheckError};

fn setup_repo(dir: &std::path::Path) {
    yunta_testkit::init_repo(dir);
    yunta_testkit::write(&dir.join("tracked.txt"), "original\n");
    yunta_testkit::git(dir, &["add", "."]);
    yunta_testkit::git(dir, &["commit", "-q", "-m", "tracked"]);
}

#[tokio::test]
async fn a_modified_tracked_file_inside_scope_is_not_a_violation() {
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "changed\n").unwrap();

    let result = scope_check(dir.path(), &["tracked.txt".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(result.diff, vec![std::path::PathBuf::from("tracked.txt")]);
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn a_new_untracked_file_outside_scope_is_a_violation() {
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::write(dir.path().join("new_file.txt"), "surprise\n").unwrap();

    let result = scope_check(dir.path(), &["tracked.txt".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(
        result.violations,
        vec![std::path::PathBuf::from("new_file.txt")]
    );
}

#[tokio::test]
async fn a_recursive_glob_covers_nested_paths() {
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("src/sub")).unwrap();
    std::fs::write(dir.path().join("src/sub/mod.rs"), "// new\n").unwrap();

    let result = scope_check(dir.path(), &["src/**".to_string()], &[])
        .await
        .unwrap();
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn no_changes_means_no_diff_and_no_violations() {
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());

    let result = scope_check(dir.path(), &["tracked.txt".to_string()], &[])
        .await
        .unwrap();
    assert!(result.diff.is_empty());
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn an_invalid_glob_is_a_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());

    let result = scope_check(dir.path(), &["[".to_string()], &[]).await;
    assert!(matches!(result, Err(ScopeCheckError::InvalidGlob { .. })));
}

#[tokio::test]
async fn a_non_git_directory_surfaces_a_typed_git_failure() {
    let dir = tempfile::tempdir().unwrap();
    // deliberately no `git init`

    let result = scope_check(dir.path(), &["**".to_string()], &[]).await;
    assert!(matches!(result, Err(ScopeCheckError::GitFailed { .. })));
}

#[tokio::test]
async fn a_path_an_adapter_staged_is_never_charged_to_scope() {
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join(".claude/skills")).unwrap();
    std::fs::write(dir.path().join(".claude/skills/review"), "a mount\n").unwrap();

    // Nothing declared staged: the file is a change outside scope like
    // any other.
    let result = scope_check(dir.path(), &["src/**".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(
        result.violations,
        vec![std::path::PathBuf::from(".claude/skills/review")]
    );

    // The adapter that ran declared exactly that path: scope leaves it
    // out, and nothing else.
    let staged = vec![std::path::PathBuf::from(".claude/skills/review")];
    let result = scope_check(dir.path(), &["src/**".to_string()], &staged)
        .await
        .unwrap();
    assert!(result.violations.is_empty(), "got: {:?}", result.violations);
    assert_eq!(
        result.diff,
        vec![std::path::PathBuf::from(".claude/skills/review")],
        "the diff still records the file; only the verdict leaves it out"
    );
}

#[tokio::test]
async fn star_does_not_cross_directories() {
    // A single `*` never crosses a `/`: `src/*.rs` covers `src/lib.rs`
    // but not a file one directory deeper, which is therefore a
    // violation of a scope that only declared the top level.
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("src/sub")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "// top\n").unwrap();
    std::fs::write(dir.path().join("src/sub/deep.rs"), "// nested\n").unwrap();

    let result = scope_check(dir.path(), &["src/*.rs".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(
        result.violations,
        vec![std::path::PathBuf::from("src/sub/deep.rs")],
        "the nested file is outside `src/*.rs`; the top-level one is not"
    );
}

#[tokio::test]
async fn non_ascii_paths_match_their_globs() {
    // `-z` turns off git's path quoting, so a non-ASCII path reaches the
    // globs byte-for-byte (`src/café.rs`) and matches `src/*.rs` — not as
    // the escaped `"src/caf\303\251.rs"` string no glob would match.
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/café.rs"), "// unicode\n").unwrap();

    let result = scope_check(dir.path(), &["src/*.rs".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(
        result.diff,
        vec![std::path::PathBuf::from("src/café.rs")],
        "the non-ASCII path is reported unescaped"
    );
    assert!(
        result.violations.is_empty(),
        "and it matches `src/*.rs`, so it is not a violation: {:?}",
        result.violations
    );
}

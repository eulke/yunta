use std::path::Path;

use yunta_engine::{scope_check, ScopeCheckError};

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join("tracked.txt"), "original\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

#[tokio::test]
async fn a_modified_tracked_file_inside_scope_is_not_a_violation() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
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
    init_repo(dir.path());
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
    init_repo(dir.path());
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
    init_repo(dir.path());

    let result = scope_check(dir.path(), &["tracked.txt".to_string()], &[])
        .await
        .unwrap();
    assert!(result.diff.is_empty());
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn an_invalid_glob_is_a_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

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
    init_repo(dir.path());
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

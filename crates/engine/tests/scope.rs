//! The scope audit over a real git checkout: what a unit of work
//! changed since the tree it started from, compared against the globs it
//! declared.
//!
//! A change inside scope is no violation, a file outside it is, a glob
//! means exactly what it says about directory boundaries and non-ASCII
//! paths, and a path the adapter staged is never charged to the unit.
//! What was already there when the unit began belongs to whoever left
//! it, which is what the starting tree is for.

use yunta_engine::{audit, head_tree, ScopeCheckError};
use yunta_testkit::Owner;

/// The audit every case here runs: from the tree the checkout stands at
/// now, which for a repository with nothing dirty is where it began.
///
/// The private index goes in a directory of its own — outside the
/// checkout, because an index kept inside the tree it measures ends up
/// in the diff it produces, and its own per-call path, because two
/// captures sharing one index fight over its lock.
async fn from_here(
    dir: &std::path::Path,
    scope: &[yunta_core::ScopeGlob],
    staged: &[std::path::PathBuf],
    owner: &Owner,
) -> Result<yunta_engine::ScopeCheckResult, ScopeCheckError> {
    let from = head_tree(dir, owner.supervision()).await?;
    let scratch = tempfile::tempdir().expect("a scratch outside the checkout");
    let index = scratch.path().join("index");
    audit(dir, &from, &index, scope, staged, owner.supervision()).await
}

fn setup_repo(dir: &std::path::Path) {
    yunta_testkit::init_repo(dir);
    yunta_testkit::write(&dir.join("tracked.txt"), "original\n");
    yunta_testkit::git(dir, &["add", "."]);
    yunta_testkit::git(dir, &["commit", "-q", "-m", "tracked"]);
}

#[tokio::test]
async fn a_modified_tracked_file_inside_scope_is_not_a_violation() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::write(dir.path().join("tracked.txt"), "changed\n").unwrap();

    let result = from_here(dir.path(), &["tracked.txt".into()], &[], &owner)
        .await
        .unwrap();
    assert_eq!(result.diff, vec![std::path::PathBuf::from("tracked.txt")]);
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn a_new_untracked_file_outside_scope_is_a_violation() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::write(dir.path().join("new_file.txt"), "surprise\n").unwrap();

    let result = from_here(dir.path(), &["tracked.txt".into()], &[], &owner)
        .await
        .unwrap();
    assert_eq!(
        result.violations,
        vec![std::path::PathBuf::from("new_file.txt")]
    );
}

#[tokio::test]
async fn a_recursive_glob_covers_nested_paths() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("src/sub")).unwrap();
    std::fs::write(dir.path().join("src/sub/mod.rs"), "// new\n").unwrap();

    let result = from_here(dir.path(), &["src/**".into()], &[], &owner)
        .await
        .unwrap();
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn no_changes_means_no_diff_and_no_violations() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());

    let result = from_here(dir.path(), &["tracked.txt".into()], &[], &owner)
        .await
        .unwrap();
    assert!(result.diff.is_empty());
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn a_non_git_directory_surfaces_a_typed_git_failure() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    // deliberately no `git init`

    let result = from_here(dir.path(), &["**".into()], &[], &owner).await;
    assert!(matches!(result, Err(ScopeCheckError::GitFailed { .. })));
}

#[tokio::test]
async fn a_path_an_adapter_staged_is_never_charged_to_scope() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join(".claude/skills")).unwrap();
    std::fs::write(dir.path().join(".claude/skills/review"), "a mount\n").unwrap();

    // Nothing declared staged: the file is a change outside scope like
    // any other.
    let result = from_here(dir.path(), &["src/**".into()], &[], &owner)
        .await
        .unwrap();
    assert_eq!(
        result.violations,
        vec![std::path::PathBuf::from(".claude/skills/review")]
    );

    // The adapter that ran declared exactly that path: scope leaves it
    // out, and nothing else.
    let staged = vec![std::path::PathBuf::from(".claude/skills/review")];
    let result = from_here(dir.path(), &["src/**".into()], &staged, &owner)
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
    let owner = Owner::new();
    // A single `*` never crosses a `/`: `src/*.rs` covers `src/lib.rs`
    // but not a file one directory deeper, which is therefore a
    // violation of a scope that only declared the top level.
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("src/sub")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "// top\n").unwrap();
    std::fs::write(dir.path().join("src/sub/deep.rs"), "// nested\n").unwrap();

    let result = from_here(dir.path(), &["src/*.rs".into()], &[], &owner)
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
    let owner = Owner::new();
    // `-z` turns off git's path quoting, so a non-ASCII path reaches the
    // globs byte-for-byte (`src/café.rs`) and matches `src/*.rs` — not as
    // the escaped `"src/caf\303\251.rs"` string no glob would match.
    let dir = tempfile::tempdir().unwrap();
    setup_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/café.rs"), "// unicode\n").unwrap();

    let result = from_here(dir.path(), &["src/*.rs".into()], &[], &owner)
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

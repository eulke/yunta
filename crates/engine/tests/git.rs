//! How a failed `git` invocation reads, on the engine's own [`GitError`]
//! and on the two modules that map it into an error of their own. Git
//! exits non-zero saying nothing often enough — a `diff` in a directory
//! that is not a repository, a `worktree remove` on a path already
//! gone — that what those messages do with an empty stderr is behavior,
//! not an edge case.
//!
//! Beside it, what the engine reads of a repository through git: which
//! files a commit holds that name a word, and their text.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::describe;
use yunta_engine::git::{blobs, words_in, Cause, GitError, Invocation};
use yunta_engine::process::Supervision;
use yunta_engine::scope_expansion::ScopeExpansionError;
use yunta_engine::{RunError, ScopeCheckError, WorktreeError};

#[path = "git/numstat.rs"]
mod numstat;

/// A git run in `/repo` with the given arguments.
fn invocation(args: &[&str]) -> Invocation {
    Invocation {
        dir: PathBuf::from("/repo"),
        args: args.iter().map(|arg| arg.to_string()).collect(),
    }
}

fn exited_non_zero(stderr: &str) -> GitError {
    GitError {
        invocation: invocation(&["status", "--porcelain"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: stderr.to_string(),
        },
    }
}

#[test]
fn a_git_failure_that_wrote_to_stderr_reads_as_what_ran_and_what_git_said() {
    assert_eq!(
        exited_non_zero("fatal: not a git repository").to_string(),
        "git status --porcelain in `/repo` failed: fatal: not a git repository"
    );
}

#[test]
fn a_git_failure_that_said_nothing_carries_no_colon_promising_a_reason() {
    assert_eq!(
        exited_non_zero("").to_string(),
        "git status --porcelain in `/repo` failed"
    );
    assert_eq!(
        exited_non_zero("\n").to_string(),
        "git status --porcelain in `/repo` failed"
    );
}

#[test]
fn a_scope_check_git_failure_that_said_nothing_names_the_command_alone() {
    let error = ScopeCheckError::Git(GitError {
        invocation: invocation(&["diff", "--name-only", "HEAD"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: String::new(),
        },
    });
    assert_eq!(
        error.to_string(),
        "git diff --name-only HEAD in `/repo` failed"
    );
}

#[test]
fn a_scope_expansion_git_failure_reads_exactly_as_a_scope_check_one() {
    let expansion = ScopeExpansionError::Git(GitError {
        invocation: invocation(&["diff", "--name-only", "HEAD"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: "fatal: bad revision".to_string(),
        },
    });
    let check = ScopeCheckError::Git(GitError {
        invocation: invocation(&["diff", "--name-only", "HEAD"]),
        cause: Cause::Exit {
            code: Some(128),
            stderr: "fatal: bad revision".to_string(),
        },
    });
    assert_eq!(expansion.to_string(), check.to_string());
    assert_eq!(
        expansion.to_string(),
        "git diff --name-only HEAD in `/repo` failed: fatal: bad revision"
    );
}

/// A git failure that crosses a module boundary keeps the `GitError` it
/// came from rather than a copy of its words: the sentence is composed
/// once, by `GitError` itself, and a module that carries it adds nothing
/// and loses nothing.
#[test]
fn a_git_failure_keeps_its_cause() {
    let sentence = exited_non_zero("fatal: not a git repository").to_string();

    let run: RunError = exited_non_zero("fatal: not a git repository").into();
    assert_eq!(describe(&run), sentence);

    let worktree: WorktreeError = exited_non_zero("fatal: not a git repository").into();
    assert_eq!(describe(&worktree), sentence);
}

/// The spawn that never ran keeps the `io::Error` that stopped it as its
/// cause: the sentence is written once, from the error itself, rather than
/// retyped by whoever reports it.
#[test]
fn a_git_that_never_ran_keeps_the_io_error_under_it() {
    let run: RunError = GitError {
        invocation: invocation(&["status", "--porcelain"]),
        cause: Cause::Spawn(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file or directory",
        )),
    }
    .into();
    assert_eq!(
        describe(&run),
        "git status --porcelain in `/repo` failed: no such file or directory"
    );
}

/// A repository with one commit, under a directory the test owns.
fn a_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    (root, repo)
}

#[tokio::test]
async fn a_linked_worktree_names_the_repository_its_main_checkout_names() {
    let owner = yunta_testkit::Owner::new();
    let (root, repo) = a_repo();
    let linked = root.path().join("linked");
    yunta_testkit::git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "side",
        ],
    );

    let main = yunta_engine::git::common_dir(&repo, owner.supervision())
        .await
        .unwrap();
    let from_linked = yunta_engine::git::common_dir(&linked, owner.supervision())
        .await
        .unwrap();
    assert_eq!(main, from_linked);
    assert!(main.is_absolute(), "{}", main.display());
}

#[tokio::test]
async fn run_branches_lists_every_run_branch_in_one_call() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo) = a_repo();
    let first = yunta_core::RunId::from("01JQ0000000000000000000001");
    let second = yunta_core::RunId::from("01JQ0000000000000000000002");
    for branch in [
        first.branch(),
        second.branch(),
        "yunta/unit/01JQ0000000000000000000001/task/t1/1".to_string(),
        "feature/unrelated".to_string(),
    ] {
        yunta_testkit::git(&repo, &["branch", &branch]);
    }

    let runs = yunta_engine::git::run_branches(&repo, owner.supervision())
        .await
        .unwrap();
    assert_eq!(runs.into_iter().collect::<Vec<_>>(), vec![first, second]);
}

#[tokio::test]
async fn a_git_that_cannot_start_keeps_its_spawn_error_as_the_cause() {
    let empty = tempfile::tempdir().unwrap();
    let env = [("PATH".to_string(), empty.path().display().to_string())];
    let cancel = CancellationToken::new();
    let supervision = Supervision {
        env: &env,
        ..Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock)
    };
    let err = yunta_engine::git::read(empty.path(), &["status"], supervision)
        .await
        .unwrap_err();
    assert!(
        matches!(err.cause, Cause::Spawn(ref e) if e.kind() == std::io::ErrorKind::NotFound),
        "got {err:?}"
    );
}

#[tokio::test]
async fn a_cancelled_scope_check_is_not_a_git_failure() {
    let repo = tempfile::tempdir().unwrap();
    yunta_testkit::init_repo(repo.path());
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = yunta_engine::changed_between(
        repo.path(),
        "HEAD",
        "HEAD",
        Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ScopeCheckError::Git(ref e) if e.cancelled()),
        "got {err:?}"
    );
}

/// A repository whose last commit holds `files`, each a path and its bytes,
/// with that commit's id.
fn a_repo_holding(files: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf, String) {
    let (root, repo) = a_repo();
    for (path, bytes) in files {
        let path = repo.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
    }
    yunta_testkit::git(&repo, &["add", "."]);
    yunta_testkit::git(&repo, &["commit", "-q", "-m", "files"]);
    let head = yunta_testkit::git_output(&repo, &["rev-parse", "HEAD"]);
    (root, repo, head)
}

/// A Rust file at the top that declares `build_it`, one below it that calls
/// `build_it` and `other`, and a Python file that calls `build_it`.
fn a_repo_naming_build_it() -> (tempfile::TempDir, PathBuf, String) {
    a_repo_holding(&[
        ("a.rs", b"pub fn build_it() {}".as_slice()),
        ("sub/b.rs", b"fn b() { build_it(); other(); }".as_slice()),
        ("c.py", b"build_it()".as_slice()),
    ])
}

/// What a search for `build_it`, `other` and `absent` among the Rust files
/// of [`a_repo_naming_build_it`] finds: no file names `absent`.
fn build_it_and_other_named() -> BTreeMap<String, BTreeSet<String>> {
    BTreeMap::from([
        (
            "build_it".to_string(),
            BTreeSet::from(["a.rs".to_string(), "sub/b.rs".to_string()]),
        ),
        (
            "other".to_string(),
            BTreeSet::from(["sub/b.rs".to_string()]),
        ),
    ])
}

#[tokio::test]
async fn one_search_names_every_word_s_files_from_the_top_of_the_repository() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo, head) = a_repo_naming_build_it();

    let named = words_in(
        &repo.join("sub"),
        &head,
        &["build_it", "other", "absent"],
        &["*.rs".to_string()],
        owner.supervision(),
    )
    .await
    .unwrap();

    assert_eq!(named, build_it_and_other_named());
}

#[tokio::test]
async fn a_search_that_finds_nothing_is_an_empty_answer() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo, head) = a_repo_naming_build_it();

    let named = words_in(
        &repo,
        &head,
        &["absent"],
        &["*.rs".to_string()],
        owner.supervision(),
    )
    .await
    .unwrap();

    assert_eq!(named, BTreeMap::new());
}

/// Adds to `repo` a submodule, cloned from a repository beside it under
/// `root`, whose Rust file calls `build_it`, and answers with the commit
/// that holds it.
fn with_a_submodule_calling_build_it(root: &Path, repo: &Path) -> String {
    let sdk = root.join("sdk");
    std::fs::create_dir_all(&sdk).unwrap();
    yunta_testkit::init_repo(&sdk);
    std::fs::write(sdk.join("lib.rs"), "fn sdk() { build_it(); }\n").unwrap();
    yunta_testkit::git(&sdk, &["add", "."]);
    yunta_testkit::git(&sdk, &["commit", "-q", "-m", "sdk"]);
    // git clones a submodule over the file protocol only when told it may.
    let allowed = "protocol.file.allow=always";
    let from = sdk.to_str().unwrap();
    let add = ["-c", allowed, "submodule", "add", "-q", from, "vendor/sdk"];
    yunta_testkit::git(repo, &add);
    yunta_testkit::git(repo, &["commit", "-q", "-m", "sdk"]);
    yunta_testkit::git_output(repo, &["rev-parse", "HEAD"])
}

/// The settings hold `submodule.recurse` too, and the repository a
/// submodule whose Rust file calls `build_it`: the search stays in the
/// repository's own files.
#[tokio::test]
async fn the_person_s_grep_settings_do_not_change_the_answer() {
    let owner = yunta_testkit::Owner::new();
    let (root, repo, _) = a_repo_naming_build_it();
    let head = with_a_submodule_calling_build_it(root.path(), &repo);
    for (name, value) in [
        ("grep.lineNumber", "true"),
        ("grep.column", "true"),
        ("color.grep", "always"),
        ("submodule.recurse", "true"),
    ] {
        yunta_testkit::git(&repo, &["config", name, value]);
    }

    let named = words_in(
        &repo.join("sub"),
        &head,
        &["build_it", "other", "absent"],
        &["*.rs".to_string()],
        owner.supervision(),
    )
    .await
    .unwrap();

    assert_eq!(named, build_it_and_other_named());
}

/// What git's attributes say of a file's diff does not keep it out of
/// the search.
#[tokio::test]
async fn a_file_its_attributes_call_binary_is_searched_all_the_same() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo, head) = a_repo_holding(&[
        (".gitattributes", b"*.rs -diff\n".as_slice()),
        ("a.rs", b"pub fn build_it() {}\n".as_slice()),
    ]);

    let named = words_in(
        &repo,
        &head,
        &["build_it"],
        &["*.rs".to_string()],
        owner.supervision(),
    )
    .await
    .unwrap();

    let only = BTreeSet::from(["a.rs".to_string()]);
    assert_eq!(named, BTreeMap::from([("build_it".to_string(), only)]));
}

/// A file with a NUL in it is named like any other, and what git prints
/// of it leaves the next file's records whole.
#[tokio::test]
async fn a_binary_file_is_named_and_the_file_after_it_is_read_whole() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo, head) = a_repo_holding(&[
        ("bin.rs", b"build_it\0".as_slice()),
        ("z.rs", b"fn z() { build_it(); other(); }\n".as_slice()),
    ]);

    let named = words_in(
        &repo,
        &head,
        &["build_it", "other"],
        &["*.rs".to_string()],
        owner.supervision(),
    )
    .await
    .unwrap();

    let both = BTreeSet::from(["bin.rs".to_string(), "z.rs".to_string()]);
    let text = BTreeSet::from(["z.rs".to_string()]);
    assert_eq!(
        named,
        BTreeMap::from([("build_it".to_string(), both), ("other".to_string(), text)])
    );
}

#[tokio::test]
async fn one_batch_reads_each_file_and_leaves_out_what_the_commit_lacks() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo, head) = a_repo_holding(&[
        ("a.rs", b"fn a() {}\n".as_slice()),
        ("b.txt", b"\x66\xff\x0a".as_slice()),
        ("with space.rs", b"x\n".as_slice()),
    ]);
    let asked = ["a.rs", "b.txt", "with space.rs", "missing.rs"].map(String::from);

    let read = blobs(&repo, &head, &asked, owner.supervision())
        .await
        .unwrap();

    assert_eq!(
        read,
        BTreeMap::from([
            ("a.rs".to_string(), "fn a() {}\n".to_string()),
            ("b.txt".to_string(), "f\u{FFFD}\n".to_string()),
            ("with space.rs".to_string(), "x\n".to_string()),
        ])
    );
}

/// A batch reads its paths a line each, so a path that holds a line break
/// is read alone; the search names it whole all the same.
#[tokio::test]
async fn a_path_that_holds_a_line_break_is_named_and_read_whole() {
    let owner = yunta_testkit::Owner::new();
    let (_root, repo, head) = a_repo_holding(&[
        ("two\nlines.rs", b"fn build_it() {}\n".as_slice()),
        ("a.rs", b"fn a() {}\n".as_slice()),
    ]);
    let asked = ["two\nlines.rs", "a.rs"].map(String::from);

    let named = words_in(
        &repo,
        &head,
        &["build_it"],
        &["*.rs".to_string()],
        owner.supervision(),
    )
    .await
    .unwrap();
    let read = blobs(&repo, &head, &asked, owner.supervision())
        .await
        .unwrap();

    let only = BTreeSet::from(["two\nlines.rs".to_string()]);
    assert_eq!(named, BTreeMap::from([("build_it".to_string(), only)]));
    assert_eq!(
        read,
        BTreeMap::from([
            (
                "two\nlines.rs".to_string(),
                "fn build_it() {}\n".to_string()
            ),
            ("a.rs".to_string(), "fn a() {}\n".to_string()),
        ])
    );
}

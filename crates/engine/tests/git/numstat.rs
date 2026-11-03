//! How a change reads as git counts it from a commit to a tree: a renamed
//! file counted once, a binary file as a file with no lines.

use tokio_util::sync::CancellationToken;
use yunta_core::events::FileChange;
use yunta_engine::git::{numstat, Numstat};
use yunta_engine::process::Supervision;
use yunta_testkit::{git, git_output, init_repo, write};
use yunta_testkit_core::FixedClock;

/// A change from a commit to a tree counts each file it touches once, a
/// renamed one too, with the lines it puts in and takes out, and a binary
/// file as a file with no lines.
#[tokio::test]
async fn numstat_counts_a_rename_once_and_a_binary_without_lines() {
    let counted = counted_renamed_and_binary_change().await;

    assert_eq!((counted.files, counted.added, counted.removed), (3, 2, 1));
}

/// The files a change moves most come first, each with the lines it moves:
/// the file with lines first, then the renamed and the binary file, which
/// move none, by their paths.
#[tokio::test]
async fn numstat_names_the_files_that_move_most_first() {
    let counted = counted_renamed_and_binary_change().await;

    assert_eq!(
        counted.largest,
        vec![
            FileChange {
                path: "a.txt".into(),
                added: 2,
                removed: 1,
            },
            FileChange {
                path: "c.txt".into(),
                added: 0,
                removed: 0,
            },
            FileChange {
                path: "logo.bin".into(),
                added: 0,
                removed: 0,
            },
        ]
    );
}

/// Counts a repository's change from its base commit to a tree that edits one
/// file, renames another and adds a binary file.
async fn counted_renamed_and_binary_change() -> Numstat {
    let root = tempfile::tempdir().unwrap();
    init_repo(root.path());
    write(&root.path().join("a.txt"), "one\ntwo\nthree\n");
    write(
        &root.path().join("b.txt"),
        "keep\nthis\nfile\nwhole\nwhen\nit\nmoves\n",
    );
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "-q", "-m", "base"]);
    let base = git_output(root.path(), &["rev-parse", "HEAD"]);
    write(&root.path().join("a.txt"), "one\n2\nthree\nfour\n");
    git(root.path(), &["mv", "b.txt", "c.txt"]);
    std::fs::write(root.path().join("logo.bin"), [0u8, 159, 146, 150]).unwrap();
    git(root.path(), &["add", "-A"]);
    let tree = git_output(root.path(), &["write-tree"]);
    let cancel = CancellationToken::new();
    let clock = FixedClock;

    numstat(
        root.path(),
        &base,
        &tree,
        Supervision::outside_any_run(&cancel, &clock),
    )
    .await
    .unwrap()
}

//! How a change reads as git counts it from a commit to a tree: a renamed
//! file counted once, a binary file as a file with no lines.

use tokio_util::sync::CancellationToken;
use yunta_engine::git::{numstat, Numstat};
use yunta_engine::process::Supervision;
use yunta_testkit::{git, git_output, init_repo, write};
use yunta_testkit_core::FixedClock;

/// A change from a commit to a tree counts each file it touches once, a
/// renamed one too, with the lines it puts in and takes out, and a binary
/// file as a file with no lines.
#[tokio::test]
async fn numstat_counts_a_rename_once_and_a_binary_without_lines() {
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

    let counted = numstat(
        root.path(),
        &base,
        &tree,
        Supervision::outside_any_run(&cancel, &clock),
    )
    .await
    .unwrap();

    assert_eq!(
        counted,
        Numstat {
            files: 3,
            added: 2,
            removed: 1,
        }
    );
}

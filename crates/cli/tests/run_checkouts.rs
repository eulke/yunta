//! The checkout a run works in, from the command line. It comes from the
//! project's pool: a later run finds what an earlier one built there, and
//! a run whose checkout went back to the pool while it was parked works in
//! another one when it wakes, on its own branch.

use std::path::PathBuf;

use yunta_testkit::{git, git_output, init_repo, run_id_from, stdout, write, yunta_in};

/// A repository that ignores `target/`, and the state root beside it.
fn world() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    write(&repo.join(".gitignore"), "target/\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "ignore builds"]);
    let home = root.path().join("state");
    (root, repo, home)
}

/// A workflow of one `bash` node.
fn one_node(name: &str, run: &str) -> String {
    format!("name: {name}\nnodes:\n  - id: only\n    kind: bash\n    run: \"{run}\"\n")
}

/// A second run works in the checkout the first worked in, and finds the
/// build it left there.
#[test]
fn a_second_run_finds_its_run_checkout_warm() {
    let (_root, repo, home) = world();
    let builds = one_node("builds", "mkdir -p target && echo warm > target/marker");
    write(&repo.join("builds.yaml"), &builds);
    write(
        &repo.join("reads.yaml"),
        &one_node("reads", "test -f target/marker"),
    );

    let first = yunta_in!(&repo, &home, &["run", "builds.yaml"]);
    assert!(stdout(&first).contains("finished"), "{}", stdout(&first));
    let second = yunta_in!(&repo, &home, &["run", "reads.yaml"]);

    assert!(
        stdout(&second).contains("finished"),
        "the second run finds the build the first left: {}",
        stdout(&second)
    );
}

/// A parked run nobody came back to gives its checkout back; another run
/// takes it; the first wakes in a checkout of its own again, on its branch.
#[test]
fn a_run_whose_checkout_went_back_works_in_another_when_it_wakes() {
    let (_root, repo, home) = world();
    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    let parks = "name: parks\nnodes:\n  - { id: setup, kind: bash, run: \"echo hello > marker.txt\" }\n  - { id: fails, kind: bash, depends_on: [setup], run: \"exit 1\" }\n";
    write(&repo.join("parks.yaml"), parks);
    write(&repo.join("also.yaml"), &one_node("also", "exit 1"));
    let first = yunta_in!(&repo, &home, &["run", "parks.yaml"]);
    let first_id = run_id_from(&first);
    let gc = yunta_in!(&repo, &home, &["gc"]);
    assert!(gc.status.success(), "{}", stdout(&gc));
    let _second = yunta_in!(&repo, &home, &["run", "also.yaml"]);

    let _resume = yunta_in!(&repo, &home, &["resume", &first_id]);

    let checkouts = yunta_testkit::pool_checkouts(&home);
    assert_eq!(checkouts.len(), 2, "{checkouts:?}");
    let branch = format!("yunta/run/{first_id}");
    let its = checkouts
        .iter()
        .find(|checkout| git_output(checkout, &["branch", "--show-current"]).trim() == branch)
        .expect("a checkout is on the woken run's branch");
    assert!(its.join("marker.txt").exists());
}

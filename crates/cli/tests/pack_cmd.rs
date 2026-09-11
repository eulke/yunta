//! `yunta pack add/remove/list/update` end to end:
//! the real compiled binary against a local git repo standing in for a
//! real pack source — no network dependency, same reasoning every other
//! `git`-touching test in this workspace uses a local repo for.

use std::path::Path;
use std::process::Output;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in, INITIAL_BRANCH};

/// Builds a minimal, valid pack repo at `dir`: `pack.yaml` (declarative,
/// no executors) plus one workflow file it lists in `contents`.
fn write_pack_v1(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         description: v1\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
}

fn write_pack_v2(dir: &Path) {
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 2.0.0\n\
         description: v2\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
}

fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack_v1(&upstream);
    commit_all(&upstream, "v1");
    git(&upstream, &["tag", "v1.0.0"]);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let home = root.path().join("state");
    (root, upstream, repo, home)
}

#[test]
fn add_vendors_the_pack_and_writes_a_lock_entry() {
    let (_root, upstream, repo, home) = setup();

    let out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("installed acme/review-pack"),
        "{}",
        stdout(&out)
    );

    let vendored = repo.join(".yunta/packs/acme/review-pack");
    assert!(vendored.join("pack.yaml").exists());
    assert!(vendored.join("workflows/review.yaml").exists());
    assert!(
        !vendored.join(".git").exists(),
        "vendored tree must not carry .git"
    );

    let lock: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    let entry = &lock["packs"]["acme/review-pack"];
    assert_eq!(entry["publisher"], "acme");
    assert_eq!(entry["name"], "review-pack");
    assert_eq!(entry["ref"], INITIAL_BRANCH);
    assert!(entry["commit"].as_str().unwrap().len() >= 7);
    assert!(!entry["content_hash"].as_str().unwrap().is_empty());
}

#[test]
fn add_with_an_explicit_ref_pins_and_records_it() {
    let (_root, upstream, repo, home) = setup();

    let source = format!("{}@v1.0.0", upstream.to_str().unwrap());
    let out = yunta_in!(&repo, &home, &["pack", "add", &source]);
    assert!(out.status.success(), "{}", stderr(&out));

    let lock: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    assert_eq!(lock["packs"]["acme/review-pack"]["ref"], "v1.0.0");
}

#[test]
fn add_refuses_a_pack_already_installed() {
    let (_root, upstream, repo, home) = setup();
    let first = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(first.status.success(), "{}", stderr(&first));

    let second = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(!second.status.success());
    assert!(
        stderr(&second).contains("already installed"),
        "{}",
        stderr(&second)
    );
}

#[test]
fn list_reports_every_locked_pack_and_verifies_it_against_the_lock() {
    let (_root, upstream, repo, home) = setup();
    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let list_out = yunta_in!(&repo, &home, &["pack", "list"]);
    assert!(list_out.status.success(), "{}", stderr(&list_out));
    let listed = stdout(&list_out);
    assert!(
        listed.contains(&format!("acme/review-pack @ {INITIAL_BRANCH}")),
        "{listed}"
    );
    assert!(
        listed
            .lines()
            .any(|l| l.starts_with("acme/review-pack @ ") && l.ends_with(") — ok")),
        "the vendored pack verifies clean against the lock: {listed}"
    );

    // Tamper with the vendored content directly (never through `add`) —
    // `list` must notice the mismatch against the lock, not just echo
    // the lock's own numbers back.
    std::fs::write(
        repo.join(".yunta/packs/acme/review-pack/workflows/review.yaml"),
        "tampered",
    )
    .unwrap();
    let list_after_tamper = yunta_in!(&repo, &home, &["pack", "list"]);
    assert!(
        stdout(&list_after_tamper).contains("MODIFIED"),
        "{}",
        stdout(&list_after_tamper)
    );
}

#[test]
fn list_with_nothing_installed_says_so_without_erroring() {
    let (_root, _upstream, repo, home) = setup();
    let out = yunta_in!(&repo, &home, &["pack", "list"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("no packs installed"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn update_revendors_at_the_new_ref_and_keeps_the_remembered_source() {
    let (_root, upstream, repo, home) = setup();
    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    write_pack_v2(&upstream);
    commit_all(&upstream, "v2");
    git(&upstream, &["tag", "v2.0.0"]);

    let update_out = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "v2.0.0"]
    );
    assert!(update_out.status.success(), "{}", stderr(&update_out));
    assert!(
        stdout(&update_out).contains("updated acme/review-pack -> v2.0.0"),
        "{}",
        stdout(&update_out)
    );

    let vendored_manifest: serde_norway::Value = serde_norway::from_str(
        &std::fs::read_to_string(repo.join(".yunta/packs/acme/review-pack/pack.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        vendored_manifest["version"], "2.0.0",
        "update re-vendors the manifest at the new version"
    );

    let lock: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    assert_eq!(lock["packs"]["acme/review-pack"]["ref"], "v2.0.0");
}

#[test]
fn update_on_an_uninstalled_pack_is_refused() {
    let (_root, _upstream, repo, home) = setup();
    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "v2.0.0"]
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("isn't installed"), "{}", stderr(&out));
}

#[test]
fn remove_deletes_the_vendored_tree_and_the_lock_entry() {
    let (_root, upstream, repo, home) = setup();
    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let remove_out = yunta_in!(&repo, &home, &["pack", "remove", "acme/review-pack"]);
    assert!(remove_out.status.success(), "{}", stderr(&remove_out));

    assert!(!repo.join(".yunta/packs/acme/review-pack").exists());
    let lock: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    assert!(lock["packs"].as_mapping().unwrap().is_empty());
}

/// A pack whose repo ships a symlink — `add` refuses it before vendoring,
/// naming the link: vendoring follows nothing, so nothing outside the
/// pack can be copied in and nothing inside can point out.
#[test]
fn add_refuses_symlinks() {
    let (_root, upstream, repo, home) = setup();
    std::os::unix::fs::symlink("/etc/hostname", upstream.join("workflows/link.yaml")).unwrap();
    commit_all(&upstream, "with a symlink");

    let out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(!out.status.success(), "{}", stdout(&out));
    assert!(
        stderr(&out).contains("symlink") && stderr(&out).contains("workflows/link.yaml"),
        "{}",
        stderr(&out)
    );
    assert!(!repo.join(".yunta/packs/acme/review-pack").exists());
    assert!(!repo.join(".yunta/yunta.lock").exists());
}

/// Writes a pack that ships an executor and a test case whose only node
/// leaves a marker file in `$YUNTA_TEST_MARKER_DIR` when it runs — the
/// one observable trace of the pack's tests having executed.
fn write_pack_with_executor_and_tests(dir: &Path) {
    std::fs::create_dir_all(dir.join(".yunta/workflows")).unwrap();
    std::fs::create_dir_all(dir.join(".yunta/tests/fixtures")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: tool-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         declares:\n  permissions: edit\n  network: false\n  executors: [lint]\n\
         contents:\n  workflows: [.yunta/workflows/mark.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join(".yunta/workflows/mark.yaml"),
        "name: mark\nnodes:\n  - id: mark\n    kind: bash\n    run: \"touch \\\"$YUNTA_TEST_MARKER_DIR/ran\\\"\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join(".yunta/config.yaml"),
        "project:\n  base_branch: master\n",
    )
    .unwrap();
    std::fs::write(
        dir.join(".yunta/tests/mark.yaml"),
        "workflow: mark\nfixture: fixtures/mark.yaml\nexpect:\n  final_state: finished\n",
    )
    .unwrap();
    std::fs::write(
        dir.join(".yunta/tests/fixtures/mark.yaml"),
        "sessions: []\n",
    )
    .unwrap();
}

fn yunta_with_marker(dir: &Path, home: &Path, marker_dir: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .env("YUNTA_TEST_MARKER_DIR", marker_dir)
        .output()
        .expect("failed to run the yunta binary")
}

/// A pack that ships executors needs `--yes`; until it is given, nothing
/// of the pack runs — its own test cases included.
#[test]
fn add_never_executes_before_confirmation() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack_with_executor_and_tests(&upstream);
    commit_all(&upstream, "v1");
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    let marker = root.path().join("marker");
    std::fs::create_dir_all(&marker).unwrap();

    let refused = yunta_with_marker(
        &repo,
        &home,
        &marker,
        &["pack", "add", "--run-tests", upstream.to_str().unwrap()],
    );
    assert!(!refused.status.success(), "{}", stdout(&refused));
    assert!(stderr(&refused).contains("--yes"), "{}", stderr(&refused));
    assert!(
        !marker.join("ran").exists(),
        "the pack's tests ran before the executor confirmation"
    );
    assert!(!repo.join(".yunta/packs/acme/tool-pack").exists());

    let confirmed = yunta_with_marker(
        &repo,
        &home,
        &marker,
        &[
            "pack",
            "add",
            "--yes",
            "--run-tests",
            upstream.to_str().unwrap(),
        ],
    );
    assert!(confirmed.status.success(), "{}", stderr(&confirmed));
    assert!(
        marker.join("ran").exists(),
        "with `--yes --run-tests` the tests run: {}\n{}",
        stdout(&confirmed),
        stderr(&confirmed)
    );
    assert!(
        stdout(&confirmed).contains("tests: 1 case, 0 failed"),
        "{}",
        stdout(&confirmed)
    );
}

/// Without `--run-tests`, `add` vendors and locks without running a
/// single node of the pack: the audit is read, not executed.
#[test]
fn add_runs_the_packs_tests_only_when_asked() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack_with_executor_and_tests(&upstream);
    commit_all(&upstream, "v1");
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    let marker = root.path().join("marker");
    std::fs::create_dir_all(&marker).unwrap();

    let out = yunta_with_marker(
        &repo,
        &home,
        &marker,
        &["pack", "add", "--yes", upstream.to_str().unwrap()],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        !marker.join("ran").exists(),
        "tests ran without `--run-tests`"
    );
    assert!(
        stdout(&out).contains("tests: 1 case shipped, not run (pass --run-tests)"),
        "{}",
        stdout(&out)
    );
    assert!(repo.join(".yunta/packs/acme/tool-pack/pack.yaml").exists());
}

/// When the lock cannot be written, nothing of the install survives:
/// no vendored tree, no half-written lock.
#[test]
fn add_is_atomic_when_lock_write_fails() {
    let (_root, upstream, repo, home) = setup();
    // A directory where the lock file goes makes every write to it fail.
    std::fs::create_dir_all(repo.join(".yunta/yunta.lock")).unwrap();

    let out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(!out.status.success(), "{}", stdout(&out));
    assert!(stderr(&out).contains("yunta.lock"), "{}", stderr(&out));
    assert!(
        !repo.join(".yunta/packs/acme/review-pack").exists(),
        "the vendored tree must be rolled back when the lock cannot be written"
    );
    assert!(
        std::fs::read_dir(repo.join(".yunta/packs")).map_or(true, |mut d| d.next().is_none()),
        "no staging directory may be left behind"
    );
}

/// A manifest whose `contents` reach outside the pack, or whose
/// publisher or name is not a single path segment, is refused before
/// anything is vendored.
#[test]
fn add_refuses_a_manifest_whose_names_escape_the_pack() {
    let (_root, upstream, repo, home) = setup();
    std::fs::write(
        upstream.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [../outside/review.yaml]\n",
    )
    .unwrap();
    commit_all(&upstream, "escaping contents");
    let out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("../outside/review.yaml")
            && stderr(&out).contains("contents.workflows"),
        "{}",
        stderr(&out)
    );

    std::fs::write(
        upstream.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme/../..\n\
         version: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    commit_all(&upstream, "escaping publisher");
    let out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("publisher"), "{}", stderr(&out));
    assert!(
        !repo.join(".yunta/packs").exists()
            || std::fs::read_dir(repo.join(".yunta/packs"))
                .unwrap()
                .next()
                .is_none()
    );
}

/// `update` vendors the new ref beside the installed tree and only then
/// swaps them: a ref that cannot be vendored leaves the installed pack
/// untouched and the lock unchanged.
#[test]
fn update_keeps_the_installed_tree_when_the_new_ref_cannot_be_vendored() {
    let (_root, upstream, repo, home) = setup();
    let add = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add.status.success(), "{}", stderr(&add));
    let lock_before = std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap();

    std::os::unix::fs::symlink("/etc/hostname", upstream.join("workflows/link.yaml")).unwrap();
    commit_all(&upstream, "v2 with a symlink");
    git(&upstream, &["tag", "v2.0.0"]);

    let out = yunta_in!(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "v2.0.0"]
    );
    assert!(!out.status.success(), "{}", stdout(&out));
    assert!(
        repo.join(".yunta/packs/acme/review-pack/workflows/review.yaml")
            .exists(),
        "the installed tree must survive a failed update"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap(),
        lock_before
    );
}

#[test]
fn pack_new_produces_a_pack_that_passes_check_and_test() {
    let (_root, _upstream, repo, home) = setup();
    let out = yunta_in!(&repo, &home, &["pack", "new", "acme/demo"]);
    assert!(out.status.success(), "pack new failed: {}", stderr(&out));

    let pack = repo.join("demo");
    assert!(pack.join("pack.yaml").is_file(), "pack.yaml is written");
    assert!(pack.join("README.md").is_file(), "a README is written");
    assert!(
        pack.join(".yunta/workflows/example.yaml").is_file(),
        "a workflow is written"
    );
    assert!(
        pack.join(".yunta/tests/example.yaml").is_file(),
        "a test case is written"
    );

    // The scaffold passes its own test run, unassisted — the shape it
    // teaches is a verified one from the first command.
    let tested = yunta_in!(&repo, &home, &["test", "--dir", "demo"]);
    assert!(
        tested.status.success(),
        "the scaffolded pack must pass `yunta test`: {}",
        stderr(&tested)
    );
    assert!(
        stdout(&tested).contains("0 failed"),
        "test output: {}",
        stdout(&tested)
    );
}

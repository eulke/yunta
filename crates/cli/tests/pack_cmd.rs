//! `yunta pack add/remove/list/update` end to end:
//! the real compiled binary against a local git repo standing in for a
//! real pack source — no network dependency, same reasoning every other
//! `git`-touching test in this workspace uses a local repo for.

use std::path::Path;
use std::process::Output;

fn yunta_in(dir: &Path, home: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .output()
        .expect("failed to run the yunta binary")
}

fn git(dir: &Path, args: &[&str]) -> Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// `-b master` pins the default branch regardless of the host's own
// `init.defaultBranch` — otherwise `add_vendors_...`'s assertion on the
// recorded ref would flake between "master" and "main" per environment.
fn init_repo(dir: &Path) {
    git_ok(dir, &["init", "-q", "-b", "master"]);
    git_ok(dir, &["config", "user.email", "test@example.com"]);
    git_ok(dir, &["config", "user.name", "Test"]);
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

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
    git_ok(dir, &["add", "."]);
    git_ok(dir, &["commit", "-q", "-m", message]);
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
    git_ok(&upstream, &["tag", "v1.0.0"]);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    std::fs::write(repo.join(".gitkeep"), "").unwrap();
    commit_all(&repo, "initial");

    let home = root.path().join("state");
    (root, upstream, repo, home)
}

#[test]
fn add_vendors_the_pack_and_writes_a_lock_entry() {
    let (_root, upstream, repo, home) = setup();

    let out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
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

    let lock: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    let entry = &lock["packs"]["acme/review-pack"];
    assert_eq!(entry["publisher"], "acme");
    assert_eq!(entry["name"], "review-pack");
    assert_eq!(entry["ref"], "master");
    assert!(entry["commit"].as_str().unwrap().len() >= 7);
    assert!(!entry["content_hash"].as_str().unwrap().is_empty());
}

#[test]
fn add_with_an_explicit_ref_pins_and_records_it() {
    let (_root, upstream, repo, home) = setup();

    let source = format!("{}@v1.0.0", upstream.to_str().unwrap());
    let out = yunta_in(&repo, &home, &["pack", "add", &source]);
    assert!(out.status.success(), "{}", stderr(&out));

    let lock: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    assert_eq!(lock["packs"]["acme/review-pack"]["ref"], "v1.0.0");
}

#[test]
fn add_refuses_a_pack_already_installed() {
    let (_root, upstream, repo, home) = setup();
    let first = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(first.status.success(), "{}", stderr(&first));

    let second = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
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
    let add_out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let list_out = yunta_in(&repo, &home, &["pack", "list"]);
    assert!(list_out.status.success(), "{}", stderr(&list_out));
    let listed = stdout(&list_out);
    assert!(listed.contains("acme/review-pack @ master"), "{listed}");
    assert!(listed.contains(") — ok"), "{listed}");

    // Tamper with the vendored content directly (never through `add`) —
    // `list` must notice the mismatch against the lock, not just echo
    // the lock's own numbers back.
    std::fs::write(
        repo.join(".yunta/packs/acme/review-pack/workflows/review.yaml"),
        "tampered",
    )
    .unwrap();
    let list_after_tamper = yunta_in(&repo, &home, &["pack", "list"]);
    assert!(
        stdout(&list_after_tamper).contains("MODIFIED"),
        "{}",
        stdout(&list_after_tamper)
    );
}

#[test]
fn list_with_nothing_installed_says_so_without_erroring() {
    let (_root, _upstream, repo, home) = setup();
    let out = yunta_in(&repo, &home, &["pack", "list"]);
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
    let add_out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    write_pack_v2(&upstream);
    commit_all(&upstream, "v2");
    git_ok(&upstream, &["tag", "v2.0.0"]);

    let update_out = yunta_in(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "v2.0.0"],
    );
    assert!(update_out.status.success(), "{}", stderr(&update_out));
    assert!(
        stdout(&update_out).contains("updated acme/review-pack -> v2.0.0"),
        "{}",
        stdout(&update_out)
    );

    let vendored_manifest =
        std::fs::read_to_string(repo.join(".yunta/packs/acme/review-pack/pack.yaml")).unwrap();
    assert!(vendored_manifest.contains("version: 2.0.0"));

    let lock: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    assert_eq!(lock["packs"]["acme/review-pack"]["ref"], "v2.0.0");
}

#[test]
fn update_on_an_uninstalled_pack_is_refused() {
    let (_root, _upstream, repo, home) = setup();
    let out = yunta_in(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "v2.0.0"],
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("isn't installed"), "{}", stderr(&out));
}

#[test]
fn remove_deletes_the_vendored_tree_and_the_lock_entry() {
    let (_root, upstream, repo, home) = setup();
    let add_out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let remove_out = yunta_in(&repo, &home, &["pack", "remove", "acme/review-pack"]);
    assert!(remove_out.status.success(), "{}", stderr(&remove_out));

    assert!(!repo.join(".yunta/packs/acme/review-pack").exists());
    let lock: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(repo.join(".yunta/yunta.lock")).unwrap())
            .unwrap();
    assert!(lock["packs"].as_mapping().unwrap().is_empty());
}

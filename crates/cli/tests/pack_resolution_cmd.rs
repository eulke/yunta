//! Namespaced resolution against a real installed pack:
//! `yunta run <publisher>/<name>` and `yunta list` both go
//! through the real compiled binary, exercising `resolve_workflow`
//! wired into the actual CLI commands, not just the resolver in
//! isolation (`crates/engine/tests/catalog.rs` already covers that).

use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in};

/// A pack whose one workflow is pure `bash` — runnable with no adapter
/// configured at all, so the test only exercises resolution, nothing
/// about sessions.
fn write_pack(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         description: a runnable pack\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\ndescription: pack-provided review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
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
    write_pack(&upstream);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let home = root.path().join("state");
    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    (root, upstream, repo, home)
}

#[test]
fn yunta_run_resolves_a_bare_publisher_slash_name_against_an_installed_pack() {
    let (_root, _upstream, repo, home) = setup();

    let run_out = yunta_in!(&repo, &home, &["run", "acme/review", "--follow"]);
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    assert!(
        stdout(&run_out).contains("finished"),
        "{}",
        stdout(&run_out)
    );
}

#[test]
fn yunta_check_accepts_a_bare_publisher_slash_name_too() {
    let (_root, _upstream, repo, home) = setup();

    let check_out = yunta_in!(&repo, &home, &["check", "acme/review"]);
    assert!(check_out.status.success(), "{}", stderr(&check_out));
    assert!(stdout(&check_out).contains("OK"), "{}", stdout(&check_out));
}

#[test]
fn yunta_list_shows_both_the_repo_catalog_and_installed_pack_workflows() {
    let (_root, _upstream, repo, home) = setup();

    std::fs::create_dir_all(repo.join(".yunta/workflows")).unwrap();
    std::fs::write(
        repo.join(".yunta/workflows/local.yaml"),
        "name: local\ndescription: repo-owned\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();

    let out = yunta_in!(&repo, &home, &["list"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let listed = stdout(&out);
    assert!(
        listed.lines().any(|l| l == "local: repo-owned"),
        "the repo-owned workflow lists with its description: {listed}"
    );
    assert!(
        listed
            .lines()
            .any(|l| l == "acme/review: pack-provided review"),
        "the pack-provided workflow lists with its namespaced name and description: {listed}"
    );
}

#[test]
fn a_repo_workflow_with_the_same_namespaced_name_shadows_the_pack() {
    let (_root, _upstream, repo, home) = setup();

    // "un workflow local con el mismo nombre pisa al del pack" — a
    // repo file at the same publisher/name path wins over the pack.
    std::fs::create_dir_all(repo.join(".yunta/workflows/acme")).unwrap();
    std::fs::write(
        repo.join(".yunta/workflows/acme/review.yaml"),
        "name: review\ndescription: repo override\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();

    let check_out = yunta_in!(&repo, &home, &["check", "acme/review"]);
    assert!(check_out.status.success(), "{}", stderr(&check_out));
    let listed = stdout(&yunta_in!(&repo, &home, &["list"]));
    assert!(
        listed.lines().any(|l| l == "acme/review: repo override"),
        "the repo file shadows the pack's workflow of the same name: {listed}"
    );
    assert!(!listed.contains("pack-provided review"), "{listed}");
}

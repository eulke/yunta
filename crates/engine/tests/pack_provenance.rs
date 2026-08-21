//! The run manifest freezes which pack (and exactly which version) its
//! top-level workflow came from (RFC-0002 §7, T11.7) — pure
//! `build_manifest` behavior, no live run needed.

use std::collections::HashMap;
use std::path::Path;

use yunta_core::ConfigLayer;
use yunta_engine::build_manifest;

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
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

const LEAF: &str = "name: leaf\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n";

#[test]
fn a_repo_origin_workflow_freezes_no_pack_provenance() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let workflow_dir = repo.path().join(".yunta/workflows");
    std::fs::create_dir_all(&workflow_dir).unwrap();

    let workflow = serde_yaml::from_str(LEAF).unwrap();
    let manifest = build_manifest(
        &workflow,
        &ConfigLayer::default(),
        &workflow_dir,
        repo.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert!(manifest.pack.is_none());
}

#[test]
fn a_pack_origin_workflow_freezes_publisher_name_and_version() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let pack_dir = repo.path().join(".yunta/packs/acme/review-pack");
    write(
        &pack_dir.join("pack.yaml"),
        "name: review-pack\npublisher: acme\nversion: 1.2.3\ndeclares:\n  \
         permissions: read-only\ncontents:\n  workflows: [review.yaml]\n",
    );
    write(&pack_dir.join("review.yaml"), LEAF);

    let workflow = serde_yaml::from_str(LEAF).unwrap();
    let manifest = build_manifest(
        &workflow,
        &ConfigLayer::default(),
        &pack_dir,
        repo.path(),
        &HashMap::new(),
    )
    .unwrap();

    let provenance = manifest.pack.expect("pack-origin workflow freezes pack");
    assert_eq!(provenance.publisher, "acme");
    assert_eq!(provenance.name, "review-pack");
    assert_eq!(provenance.version, "1.2.3");
    assert!(provenance.commit.is_none(), "no yunta.lock entry exists");
}

#[test]
fn a_pack_origin_workflow_also_freezes_the_locked_commit_when_one_exists() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let pack_dir = repo.path().join(".yunta/packs/acme/review-pack");
    write(
        &pack_dir.join("pack.yaml"),
        "name: review-pack\npublisher: acme\nversion: 1.2.3\ndeclares:\n  \
         permissions: read-only\ncontents:\n  workflows: [review.yaml]\n",
    );
    write(&pack_dir.join("review.yaml"), LEAF);
    write(
        &repo.path().join(".yunta/yunta.lock"),
        "packs:\n  acme/review-pack:\n    publisher: acme\n    name: review-pack\n    \
         source: https://example.invalid/acme/review-pack\n    ref: v1.2.3\n    \
         commit: abcdef0123456789abcdef0123456789abcdef01\n    content_hash: deadbeef\n",
    );

    let workflow = serde_yaml::from_str(LEAF).unwrap();
    let manifest = build_manifest(
        &workflow,
        &ConfigLayer::default(),
        &pack_dir,
        repo.path(),
        &HashMap::new(),
    )
    .unwrap();

    let provenance = manifest.pack.expect("pack-origin workflow freezes pack");
    assert_eq!(
        provenance.commit.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef01")
    );
}

#[test]
fn a_pack_with_no_readable_manifest_freezes_no_provenance_rather_than_failing_the_run() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    // The directory exists (so origin_of reports Pack) but pack.yaml
    // itself doesn't — a self-inconsistent local state this function
    // has no better answer for than omitting provenance.
    let pack_dir = repo.path().join(".yunta/packs/acme/review-pack");
    std::fs::create_dir_all(&pack_dir).unwrap();

    let workflow = serde_yaml::from_str(LEAF).unwrap();
    let manifest = build_manifest(
        &workflow,
        &ConfigLayer::default(),
        &pack_dir,
        repo.path(),
        &HashMap::new(),
    )
    .unwrap();

    assert!(manifest.pack.is_none());
}

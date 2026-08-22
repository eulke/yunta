//! `declares.permissions` as a ceiling enforced by `check`:
//! a pack's own `prompt`/`loop` node can never request a
//! session profile above what its manifest promises — pure filesystem
//! fixtures, no live run needed.

use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{check_workflow_refs, origin_of, CheckError};

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn write_pack(root: &Path, publisher: &str, pack_name: &str, declared_permissions: &str) {
    let pack_dir = root.join(".yunta/packs").join(publisher).join(pack_name);
    write(
        &pack_dir.join("pack.yaml"),
        &format!(
            "name: {pack_name}\npublisher: {publisher}\nversion: 1.0.0\ndeclares:\n  \
             permissions: {declared_permissions}\ncontents:\n  workflows: [review.yaml]\n"
        ),
    );
}

fn pack_dir(root: &Path, publisher: &str, pack_name: &str) -> std::path::PathBuf {
    root.join(".yunta/packs").join(publisher).join(pack_name)
}

fn check(root: &Path, publisher: &str, pack_name: &str, workflow_yaml: &str) -> Vec<CheckError> {
    let path = pack_dir(root, publisher, pack_name).join("review.yaml");
    write(&path, workflow_yaml);
    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let origin = origin_of(root, &path);
    check_workflow_refs(&workflow, &ConfigLayer::default(), root, &origin)
}

#[test]
fn a_node_that_exceeds_the_packs_declared_ceiling_fails_check() {
    let root = tempfile::tempdir().unwrap();
    write_pack(root.path(), "acme", "review-pack", "read-only");

    let errors = check(
        root.path(),
        "acme",
        "review-pack",
        "name: review\nnodes:\n  - { id: draft, kind: prompt, prompt: hi, permissions: edit }\n",
    );

    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::PackPermissionsCeilingExceeded { node, pack, declared, effective }
                if node.as_str() == "draft"
                    && pack == "acme/review-pack"
                    && *declared == "read-only"
                    && *effective == "edit"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_node_with_no_explicit_permissions_still_defaults_to_edit_and_can_exceed_the_ceiling() {
    let root = tempfile::tempdir().unwrap();
    write_pack(root.path(), "acme", "review-pack", "read-only");

    let errors = check(
        root.path(),
        "acme",
        "review-pack",
        "name: review\nnodes:\n  - { id: draft, kind: prompt, prompt: hi }\n",
    );

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::PackPermissionsCeilingExceeded { .. })),
        "got: {errors:?}"
    );
}

#[test]
fn a_node_at_or_under_the_declared_ceiling_passes() {
    let root = tempfile::tempdir().unwrap();
    write_pack(root.path(), "acme", "review-pack", "read-only");

    let errors = check(
        root.path(),
        "acme",
        "review-pack",
        "name: review\nnodes:\n  - { id: draft, kind: prompt, prompt: hi, permissions: read-only }\n",
    );

    assert!(errors.is_empty(), "got: {errors:?}");
}

#[test]
fn a_pack_declaring_a_higher_ceiling_allows_edit_nodes() {
    let root = tempfile::tempdir().unwrap();
    write_pack(root.path(), "acme", "review-pack", "edit");

    let errors = check(
        root.path(),
        "acme",
        "review-pack",
        "name: review\nnodes:\n  - { id: draft, kind: prompt, prompt: hi }\n",
    );

    assert!(errors.is_empty(), "got: {errors:?}");
}

#[test]
fn a_repo_origin_workflow_has_no_ceiling_to_enforce() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".yunta/workflows/review.yaml");
    let text =
        "name: review\nnodes:\n  - { id: draft, kind: prompt, prompt: hi, permissions: full }\n";
    write(&path, text);
    let workflow: Workflow = serde_yaml::from_str(text).unwrap();
    let origin = origin_of(root.path(), &path);

    let errors = check_workflow_refs(&workflow, &ConfigLayer::default(), root.path(), &origin);
    assert!(errors.is_empty(), "got: {errors:?}");
}

#[test]
fn a_node_exceeding_the_ceiling_inside_a_composed_child_is_also_caught() {
    let root = tempfile::tempdir().unwrap();
    let dir = pack_dir(root.path(), "acme", "review-pack");
    write(
        &dir.join("pack.yaml"),
        "name: review-pack\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
         permissions: read-only\ncontents:\n  workflows: [review.yaml, qa.yaml]\n",
    );
    write(
        &dir.join("qa.yaml"),
        "name: qa\nnodes:\n  - { id: check, kind: prompt, prompt: hi, permissions: full }\n",
    );

    let parent_text = "name: review\nnodes:\n  - { id: sub, kind: workflow, use: acme/qa }\n";
    let parent_path = dir.join("review.yaml");
    write(&parent_path, parent_text);
    let parent: Workflow = serde_yaml::from_str(parent_text).unwrap();
    let origin = origin_of(root.path(), &parent_path);

    let errors = check_workflow_refs(&parent, &ConfigLayer::default(), root.path(), &origin);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::PackPermissionsCeilingExceeded { node, .. } if node.as_str() == "check"
        )),
        "got: {errors:?}"
    );
}

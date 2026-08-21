//! Namespaced resolution (RFC-0002 §5, T11.3): `resolve_workflow` and
//! its integration into `check_workflow_refs` — pure filesystem
//! fixtures, no live run needed to exercise resolution or the
//! cross-pack composition rule.

use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{
    check_workflow_refs, origin_of, resolve_workflow, CatalogError, CheckError, WorkflowOrigin,
};

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn write_pack(
    root: &Path,
    publisher: &str,
    pack_name: &str,
    pack_yaml_extra: &str,
    workflows: &[(&str, &str)],
) {
    let pack_dir = root.join(".yunta/packs").join(publisher).join(pack_name);
    let declared: Vec<String> = workflows.iter().map(|(name, _)| name.to_string()).collect();
    write(
        &pack_dir.join("pack.yaml"),
        &format!(
            "name: {pack_name}\npublisher: {publisher}\nversion: 1.0.0\ndeclares:\n  \
             permissions: read-only\ncontents:\n  workflows: [{}]\n{pack_yaml_extra}",
            declared.join(", ")
        ),
    );
    for (name, contents) in workflows {
        write(&pack_dir.join(name), contents);
    }
}

const LEAF: &str = "name: leaf\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n";

#[test]
fn a_repo_workflow_shadows_a_pack_workflow_of_the_same_name() {
    let root = tempfile::tempdir().unwrap();
    write_pack(
        root.path(),
        "acme",
        "review-pack",
        "",
        &[("review.yaml", LEAF)],
    );
    // The repo names its own "review" too — §5: "un workflow local con
    // el mismo nombre pisa al del pack."
    write(
        &root.path().join(".yunta/workflows/acme/review.yaml"),
        "name: repo-review\nnodes: []\n",
    );

    let resolved = resolve_workflow(root.path(), "acme/review").unwrap();
    assert_eq!(resolved.origin, WorkflowOrigin::Repo);
    assert!(resolved.path.ends_with(".yunta/workflows/acme/review.yaml"));
}

#[test]
fn a_bare_name_never_falls_through_to_packs() {
    let root = tempfile::tempdir().unwrap();
    // No repo workflow named "review", and no publisher segment at all
    // — there's nothing for a bare name to fall through to.
    let err = resolve_workflow(root.path(), "review").unwrap_err();
    assert!(matches!(err, CatalogError::NotFoundNoPacksDir { .. }));
}

#[test]
fn a_namespaced_name_resolves_to_the_publishers_pack_when_the_repo_has_nothing_by_that_name() {
    let root = tempfile::tempdir().unwrap();
    write_pack(
        root.path(),
        "acme",
        "review-pack",
        "",
        &[("review.yaml", LEAF)],
    );

    let resolved = resolve_workflow(root.path(), "acme/review").unwrap();
    match resolved.origin {
        WorkflowOrigin::Pack {
            publisher,
            pack_name,
        } => {
            assert_eq!(publisher, "acme");
            assert_eq!(pack_name, "review-pack");
        }
        other => panic!("expected Pack origin, got {other:?}"),
    }
    assert!(resolved.path.ends_with("acme/review-pack/review.yaml"));
}

#[test]
fn two_packs_from_the_same_publisher_declaring_the_same_workflow_basename_is_ambiguous() {
    let root = tempfile::tempdir().unwrap();
    write_pack(root.path(), "acme", "pack-a", "", &[("review.yaml", LEAF)]);
    write_pack(root.path(), "acme", "pack-b", "", &[("review.yaml", LEAF)]);

    let err = resolve_workflow(root.path(), "acme/review").unwrap_err();
    assert!(
        matches!(err, CatalogError::Ambiguous { count: 2, .. }),
        "{err:?}"
    );
}

#[test]
fn intra_pack_composition_is_allowed_in_check() {
    let root = tempfile::tempdir().unwrap();
    write_pack(
        root.path(),
        "acme",
        "review-pack",
        "",
        &[
            (
                "review.yaml",
                "name: review\nnodes:\n  - { id: sub, kind: workflow, use: acme/qa }\n",
            ),
            ("qa.yaml", LEAF),
        ],
    );

    let parent_path = root
        .path()
        .join(".yunta/packs/acme/review-pack/review.yaml");
    let parent: Workflow =
        serde_yaml::from_str(&std::fs::read_to_string(&parent_path).unwrap()).unwrap();
    let origin = origin_of(root.path(), &parent_path);
    let errors = check_workflow_refs(&parent, &ConfigLayer::default(), root.path(), &origin);
    assert!(errors.is_empty(), "got: {errors:?}");
}

#[test]
fn cross_pack_composition_is_rejected_in_check() {
    let root = tempfile::tempdir().unwrap();
    write_pack(
        root.path(),
        "acme",
        "review-pack",
        "",
        &[(
            "review.yaml",
            "name: review\nnodes:\n  - { id: sub, kind: workflow, use: other/thing }\n",
        )],
    );
    write_pack(
        root.path(),
        "other",
        "thing-pack",
        "",
        &[("thing.yaml", LEAF)],
    );

    let parent_path = root
        .path()
        .join(".yunta/packs/acme/review-pack/review.yaml");
    let parent: Workflow =
        serde_yaml::from_str(&std::fs::read_to_string(&parent_path).unwrap()).unwrap();
    let origin = origin_of(root.path(), &parent_path);
    let errors = check_workflow_refs(&parent, &ConfigLayer::default(), root.path(), &origin);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CrossPackWorkflowRef { name, from_pack, .. }
                if name == "other/thing" && from_pack == "acme/review-pack"
        )),
        "got: {errors:?}"
    );
}

#[test]
fn a_pack_workflow_referencing_back_to_the_repo_is_also_rejected() {
    let root = tempfile::tempdir().unwrap();
    write_pack(
        root.path(),
        "acme",
        "review-pack",
        "",
        &[(
            "review.yaml",
            "name: review\nnodes:\n  - { id: sub, kind: workflow, use: local-thing }\n",
        )],
    );
    write(&root.path().join(".yunta/workflows/local-thing.yaml"), LEAF);

    let parent_path = root
        .path()
        .join(".yunta/packs/acme/review-pack/review.yaml");
    let parent: Workflow =
        serde_yaml::from_str(&std::fs::read_to_string(&parent_path).unwrap()).unwrap();
    let origin = origin_of(root.path(), &parent_path);
    let errors = check_workflow_refs(&parent, &ConfigLayer::default(), root.path(), &origin);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::CrossPackWorkflowRef { .. })),
        "got: {errors:?}"
    );
}

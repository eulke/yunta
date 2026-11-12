//! Workflow lookup falls back to the global pack store after project packs.
use std::path::Path;

use yunta_engine::{resolve_workflow, WorkflowOrigin};

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn write_pack(root: &Path, pack_name: &str, workflow_contents: &str) {
    let dir = root.join("acme").join(pack_name);
    write(
        &dir.join("pack.yaml"),
        &format!(
            "name: {pack_name}\npublisher: acme\nversion: 1.0.0\ndeclares:\n  permissions: read-only\ncontents:\n  workflows: [review.yaml]\n"
        ),
    );
    write(&dir.join("review.yaml"), workflow_contents);
}

#[test]
fn global_workflow_is_fallback_and_project_pack_shadows_it() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let global = root.path().join("global-packs");
    write_pack(&global, "global-review", "name: review\ndescription: global\nnodes: []\n");

    let resolved = resolve_workflow(&repo, &global, "acme/review").unwrap();
    assert_eq!(resolved.origin, WorkflowOrigin::Pack {
        publisher: "acme".parse().unwrap(),
        pack_name: "global-review".parse().unwrap(),
    });
    assert!(resolved.path.starts_with(&global));

    write_pack(&repo.join(".yunta/packs"), "local-review", "name: review\ndescription: local\nnodes: []\n");
    let resolved = resolve_workflow(&repo, &global, "acme/review").unwrap();
    assert_eq!(resolved.origin, WorkflowOrigin::Pack {
        publisher: "acme".parse().unwrap(),
        pack_name: "local-review".parse().unwrap(),
    });
    assert!(resolved.path.starts_with(repo.join(".yunta/packs")));
}

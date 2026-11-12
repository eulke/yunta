//! Global pack management through the compiled command line.
use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in};

fn write_pack(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\npublisher: acme\nversion: 1.0.0\ndescription: shared review\ndeclares:\n  permissions: read-only\n  network: false\n  executors: []\ncontents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial pack"]);
}

#[test]
fn global_pack_operations_use_the_global_scope_and_leave_project_scope_empty() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack(&upstream);

    let project = root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    init_repo(&project);
    let home = root.path().join("user-data");

    let add = yunta_in!(&project, &home, &["pack", "add", "--global", upstream.to_str().unwrap()]);
    assert!(add.status.success(), "{}", stderr(&add));

    let global_list = yunta_in!(&project, &home, &["pack", "list", "--global"]);
    assert!(global_list.status.success(), "{}", stderr(&global_list));
    assert!(stdout(&global_list).contains("acme/review-pack"), "{}", stdout(&global_list));

    let project_list = yunta_in!(&project, &home, &["pack", "list"]);
    assert!(project_list.status.success(), "{}", stderr(&project_list));
    assert!(!stdout(&project_list).contains("acme/review-pack"), "{}", stdout(&project_list));

    let audit = yunta_in!(&project, &home, &["pack", "audit", "--global", "acme/review-pack"]);
    assert!(audit.status.success(), "{}", stderr(&audit));

    let remove = yunta_in!(&project, &home, &["pack", "remove", "--global", "acme/review-pack"]);
    assert!(remove.status.success(), "{}", stderr(&remove));
    let empty_global = yunta_in!(&project, &home, &["pack", "list", "--global"]);
    assert!(empty_global.status.success(), "{}", stderr(&empty_global));
    assert!(!stdout(&empty_global).contains("acme/review-pack"), "{}", stdout(&empty_global));
}

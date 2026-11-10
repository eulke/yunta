use std::path::Path;
use std::sync::Arc;

use yunta_core::{Env, ProcessSecrets, SystemIdSource};
use yunta_testkit_core::FixedClock;

use crate::Workspace;

/// A workspace working in `dir`, whatever directory the process runs in.
fn working_in(dir: &Path) -> Workspace {
    Workspace {
        dir: dir.to_path_buf(),
        env: Env::default(),
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        binary: "yunta".into(),
    }
}

/// A workflow and a config named by a relative path are the workspace's:
/// read from its directory, not the process's, and named as they were
/// written.
#[test]
fn a_relative_path_is_read_from_the_workspace_and_named_as_written() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("wf.yaml"),
        "name: wf\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("merged.yaml"), "project:\n  name: here\n").unwrap();
    let workspace = working_in(dir.path());

    let read = workspace.read_workflow(Path::new("wf.yaml")).unwrap();
    let audited = workspace.audit_workflow(Path::new("wf.yaml")).unwrap();
    let config = workspace.read_config(Path::new("merged.yaml")).unwrap();

    assert_eq!(read.path, Path::new("wf.yaml"));
    assert_eq!(read.workflow.name.as_str(), "wf");
    assert_eq!(audited.workflow, read.workflow);
    assert_eq!(config.project.unwrap().name.as_deref(), Some("here"));
}

/// A file that is not there is named as it was written.
#[test]
fn a_missing_workflow_is_named_as_written() {
    let dir = tempfile::tempdir().unwrap();
    let missing = working_in(dir.path())
        .read_workflow(Path::new("gone.yaml"))
        .err()
        .unwrap();
    assert!(
        missing
            .to_string()
            .starts_with("failed to read workflow at gone.yaml: "),
        "{missing}"
    );
}

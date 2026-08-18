use std::path::Path;

use yunta_core::{sha256_hex, Node};
use yunta_engine::{close_artifacts, ArtifactError};

fn node(yaml: &str) -> Node {
    serde_yaml::from_str(yaml).unwrap()
}

fn write_artifact(run_dir: &Path, name: &str, content: &str) {
    let dir = run_dir.join("artifacts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), content).unwrap();
}

const PLAN_NODE: &str = r#"
id: plan
kind: prompt
prompt: "Write the ledger."
artifacts:
  produces:
    - { name: plan.yaml, kind: task-ledger }
"#;

const VALID_LEDGER: &str = r#"
tasks:
  - id: T001
    title: "First task"
    scope: ["src/a/"]
    criteria:
      - cmd: "test -f src/a/done"
  - id: T002
    title: "Second task"
    scope: ["src/b/"]
    criteria:
      - cmd: "test -f src/b/done"
    depends_on: [T001]
"#;

#[test]
fn a_missing_declared_artifact_fails_the_node_no_matter_what_the_agent_said() {
    let run_dir = tempfile::tempdir().unwrap();
    let n = node(
        r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#,
    );

    let errors = close_artifacts(&n, run_dir.path()).unwrap_err();
    match &errors[..] {
        [ArtifactError::Missing { node, name }] => {
            assert_eq!(node.as_str(), "report");
            assert_eq!(name, "report.md");
        }
        other => panic!("expected one Missing error, got {other:?}"),
    }
}

#[test]
fn an_empty_artifact_is_as_bad_as_a_missing_one() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report.md", "");
    let n = node(
        r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#,
    );

    let errors = close_artifacts(&n, run_dir.path()).unwrap_err();
    assert!(matches!(&errors[..], [ArtifactError::Empty { .. }]));
}

#[test]
fn every_missing_artifact_is_reported_not_just_the_first() {
    let run_dir = tempfile::tempdir().unwrap();
    let n = node(
        r#"
id: report
kind: prompt
prompt: "Write both."
artifacts:
  produces: [one.md, two.md]
"#,
    );

    let errors = close_artifacts(&n, run_dir.path()).unwrap_err();
    assert_eq!(errors.len(), 2);
}

#[test]
fn an_opaque_artifact_is_verified_by_existence_and_hash_never_by_format() {
    let run_dir = tempfile::tempdir().unwrap();
    // Content that is not valid YAML/JSON/anything — §4: the engine
    // assumes no format for opaque artifacts.
    write_artifact(run_dir.path(), "report.md", "{{{ not : parseable ][");
    let n = node(
        r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#,
    );

    let verified = close_artifacts(&n, run_dir.path()).unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].name, "report.md");
    assert_eq!(
        verified[0].content_hash,
        sha256_hex("{{{ not : parseable ][".as_bytes())
    );
    assert!(verified[0].ledger.is_none());
}

#[test]
fn a_valid_task_ledger_is_parsed_and_returned_for_registration() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "plan.yaml", VALID_LEDGER);

    let verified = close_artifacts(&node(PLAN_NODE), run_dir.path()).unwrap();
    let ledger = verified[0].ledger.as_ref().expect("a parsed ledger");
    let ids: Vec<&str> = ledger.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["T001", "T002"]);
}

#[test]
fn an_invalid_ledger_reports_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    // Two independent violations: T001 has no criteria, T002 has an
    // empty scope. Both must surface in one pass (spec-ledger §4).
    write_artifact(
        run_dir.path(),
        "plan.yaml",
        r#"
tasks:
  - id: T001
    title: "No criteria"
    scope: ["src/a/"]
    criteria: []
  - id: T002
    title: "No scope"
    scope: []
    criteria:
      - cmd: "test -f x"
"#,
    );

    let errors = close_artifacts(&node(PLAN_NODE), run_dir.path()).unwrap_err();
    match &errors[..] {
        [ArtifactError::InvalidLedger { node, name, errors }] => {
            assert_eq!(node.as_str(), "plan");
            assert_eq!(name, "plan.yaml");
            assert_eq!(errors.len(), 2);
        }
        other => panic!("expected one InvalidLedger error, got {other:?}"),
    }
}

#[test]
fn a_malformed_ledger_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "plan.yaml", "tasks: [not, a, ledger");

    let errors = close_artifacts(&node(PLAN_NODE), run_dir.path()).unwrap_err();
    assert!(matches!(
        &errors[..],
        [ArtifactError::MalformedLedger { .. }]
    ));
}

#[test]
fn a_node_that_declares_no_artifacts_verifies_nothing() {
    let run_dir = tempfile::tempdir().unwrap();
    let n = node(
        r#"
id: quiet
kind: bash
run: "true"
"#,
    );

    let verified = close_artifacts(&n, run_dir.path()).unwrap();
    assert!(verified.is_empty());
}

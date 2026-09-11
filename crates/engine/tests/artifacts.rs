use std::path::Path;

use yunta_core::diagnostic::Report;
use yunta_core::{sha256_hex, Node};
use yunta_engine::{close_artifacts, render_for_person};

/// Every rule broken across every failing artifact, by its stable code.
fn codes(reports: &[Report]) -> Vec<&str> {
    reports
        .iter()
        .flat_map(|report| report.diagnostics.iter().map(|d| d.code()))
        .collect()
}

/// The failing artifacts as a person reads them.
fn rendered(reports: &[Report]) -> String {
    render_for_person(reports)
}

fn node(yaml: &str) -> Node {
    serde_norway::from_str(yaml).unwrap()
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

    let errors = close_artifacts(&n, run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["artifact-missing"]);
    let text = rendered(&errors);
    assert!(text.contains("artifacts/report.md"), "{text}");
    assert!(text.contains("node `report`"), "{text}");
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

    let errors = close_artifacts(&n, run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["artifact-empty"]);
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

    let errors = close_artifacts(&n, run_dir.path(), None).unwrap_err();
    assert_eq!(errors.len(), 2);
}

#[test]
fn an_opaque_artifact_is_verified_by_existence_and_hash_never_by_format() {
    let run_dir = tempfile::tempdir().unwrap();
    // Content that is not valid YAML/JSON/anything — the engine
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

    let verified = close_artifacts(&n, run_dir.path(), None).unwrap();
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

    let verified = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap();
    let ledger = verified[0].ledger.as_ref().expect("a parsed ledger");
    let ids: Vec<&str> = ledger.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["T001", "T002"]);
}

#[test]
fn an_invalid_ledger_reports_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    // Two independent violations: T001 has no criteria, T002 has an
    // empty scope. Both must surface in one pass.
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

    let errors = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap_err();
    // Every violation reaches the reader, not a count of them, and in
    // document order: someone correcting a file works top to bottom.
    assert_eq!(codes(&errors), ["no-criteria", "empty-scope"]);
    let text = rendered(&errors);
    assert!(text.contains("artifacts/plan.yaml: 2 errors"), "{text}");
    assert!(text.contains("task `T001`: no criteria declared"), "{text}");
    assert!(text.contains("task `T002`: `scope` is empty"), "{text}");
}

#[test]
fn a_malformed_ledger_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "plan.yaml", "tasks: [not, a, ledger");

    let errors = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["not-yaml"]);
}

const REVIEW_NODE: &str = r#"
id: review
kind: prompt
prompt: "Review the changes."
artifacts:
  produces:
    - { name: findings.yaml, kind: findings }
"#;

const VALID_FINDINGS: &str = r#"
findings:
  - id: f1
    severity: major
    title: "Unchecked error"
    location: "src/lib.rs:10"
    detail: "The Result is discarded silently."
  - id: f2
    severity: note
    title: "Style nit"
    location: "src/lib.rs:20"
    detail: "Prefer the idiomatic form here."
"#;

#[test]
fn a_valid_findings_artifact_is_parsed_and_returned() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "findings.yaml", VALID_FINDINGS);

    let verified = close_artifacts(&node(REVIEW_NODE), run_dir.path(), None).unwrap();
    let findings = verified[0].findings.as_ref().expect("parsed findings");
    let ids: Vec<&str> = findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["f1", "f2"]);
}

#[test]
fn duplicate_finding_ids_report_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "findings.yaml",
        r#"
findings:
  - id: f1
    severity: major
    title: "First"
    location: "a.rs:1"
    detail: "detail"
  - id: f1
    severity: minor
    title: ""
    location: "b.rs:2"
    detail: "detail"
"#,
    );

    let errors = close_artifacts(&node(REVIEW_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["duplicate-id", "empty-title"]);
    let text = rendered(&errors);
    assert!(text.contains("artifacts/findings.yaml: 2 errors"), "{text}");
}

#[test]
fn a_malformed_findings_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "findings.yaml", "findings: [not, valid");

    let errors = close_artifacts(&node(REVIEW_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["not-yaml"]);
}

const ASK_NODE: &str = r#"
id: ask
kind: prompt
prompt: "Ask what you need to know."
artifacts:
  produces:
    - { name: questions.yaml, kind: questions }
"#;

const VALID_QUESTIONS: &str = r#"
questions:
  - id: q1
    text: "Which environment?"
    answer_type: choice
    values: [staging, production]
    required: true
  - id: q2
    text: "Any notes?"
    answer_type: text
    required: false
"#;

#[test]
fn a_valid_questions_artifact_is_parsed_and_returned() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "questions.yaml", VALID_QUESTIONS);

    let verified = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap();
    let questions = verified[0].questions.as_ref().expect("parsed questions");
    let ids: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["q1", "q2"]);
}

#[test]
fn a_choice_question_with_no_values_is_a_reported_violation() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "questions.yaml",
        r#"
questions:
  - id: q1
    text: "Which environment?"
    answer_type: choice
    required: true
"#,
    );

    let errors = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors).len(), 1);
    let text = rendered(&errors);
    assert!(text.contains("artifacts/questions.yaml"), "{text}");
}

#[test]
fn duplicate_question_ids_report_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "questions.yaml",
        r#"
questions:
  - id: q1
    text: "First"
    answer_type: text
    required: true
  - id: q1
    text: ""
    answer_type: text
    required: false
"#,
    );

    let errors = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["duplicate-id", "empty-text"]);
    let text = rendered(&errors);
    assert!(
        text.contains("artifacts/questions.yaml: 2 errors"),
        "{text}"
    );
}

#[test]
fn a_malformed_questions_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "questions.yaml", "questions: [not, valid");

    let errors = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&errors), ["not-yaml"]);
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

    let verified = close_artifacts(&n, run_dir.path(), None).unwrap();
    assert!(verified.is_empty());
}

// --- limits.max_artifact_bytes ------------------------------------------------

#[test]
fn an_artifact_over_max_artifact_bytes_fails_the_node_with_the_sizes_named() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report.md", "0123456789");
    let n = node(
        r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#,
    );

    let errors = close_artifacts(&n, run_dir.path(), Some(5)).unwrap_err();
    assert_eq!(codes(&errors), ["artifact-oversized"]);
    // Both numbers on the table, never a truncation.
    assert_eq!(
        rendered(&errors),
        "artifacts/report.md: 1 error\n  \
         the document is 10 bytes; `limits.max_artifact_bytes` is 5"
    );
}

#[test]
fn an_artifact_at_the_cap_or_with_no_cap_passes() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report.md", "0123456789");
    let n = node(
        r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#,
    );

    assert!(close_artifacts(&n, run_dir.path(), Some(10)).is_ok());
    assert!(close_artifacts(&n, run_dir.path(), None).is_ok());
}

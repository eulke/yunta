use std::path::Path;

use yunta_core::diagnostic::{ArtifactFailure, FileProblem};
use yunta_core::{sha256_hex, Node};
use yunta_engine::{close_artifacts, ArtifactContent};

/// Every rule broken across every failing artifact, by its stable code —
/// a problem with the file under its own code, a problem with the
/// content under the document's.
fn codes(failures: &[ArtifactFailure]) -> Vec<&str> {
    failures
        .iter()
        .flat_map(|failure| match failure {
            ArtifactFailure::File { problem, .. } => vec![problem.code()],
            ArtifactFailure::Content(report) => {
                report.diagnostics.iter().map(|d| d.code()).collect()
            }
        })
        .collect()
}

/// The failing artifacts as a person reads them: each one's own block,
/// in declaration order.
fn rendered(failures: &[ArtifactFailure]) -> String {
    failures
        .iter()
        .map(ArtifactFailure::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn node(yaml: &str) -> Node {
    serde_norway::from_str(yaml).unwrap()
}

fn write_artifact(run_dir: &Path, name: &str, content: &str) {
    let dir = run_dir.join("artifacts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), content).unwrap();
}

const REPORT_NODE: &str = r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#;

const PLAN_NODE: &str = r#"
id: plan
kind: prompt
prompt: "Write the tasks document."
artifacts:
  produces:
    - { name: plan.yaml, kind: tasks }
"#;

const VALID_TASKS: &str = r#"
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

    let failures = close_artifacts(&node(REPORT_NODE), run_dir.path(), None).unwrap_err();
    // The node that declared it is on the failure as data, not only in
    // the sentence a reader gets.
    match &failures[..] {
        [ArtifactFailure::File {
            path,
            problem: FileProblem::Missing { node },
        }] => {
            assert_eq!(path, "artifacts/report.md");
            assert_eq!(node.as_str(), "report");
        }
        other => panic!("a missing file, named: {other:?}"),
    }
    assert!(rendered(&failures).contains("artifacts/report.md"));
}

#[test]
fn an_empty_artifact_is_as_bad_as_a_missing_one() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report.md", "");

    let failures = close_artifacts(&node(REPORT_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&failures), ["artifact-empty"]);
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

    let failures = close_artifacts(&n, run_dir.path(), None).unwrap_err();
    assert_eq!(failures.len(), 2);
}

#[test]
fn an_opaque_artifact_is_verified_by_existence_and_hash_never_by_format() {
    let run_dir = tempfile::tempdir().unwrap();
    // Content that is not valid YAML/JSON/anything — the engine
    // assumes no format for opaque artifacts.
    write_artifact(run_dir.path(), "report.md", "{{{ not : parseable ][");

    let verified = close_artifacts(&node(REPORT_NODE), run_dir.path(), None).unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].name, "report.md");
    assert_eq!(
        verified[0].content_hash,
        sha256_hex("{{{ not : parseable ][".as_bytes())
    );
    assert_eq!(verified[0].content, ArtifactContent::Opaque);
    assert_eq!(verified[0].content.kind(), None);
}

#[test]
fn a_valid_tasks_document_is_parsed_and_returned_for_registration() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "plan.yaml", VALID_TASKS);

    let verified = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap();
    let ArtifactContent::Tasks(tasks) = &verified[0].content else {
        panic!("a parsed tasks document: {:?}", verified[0].content);
    };
    let ids: Vec<&str> = tasks.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["T001", "T002"]);
    assert_eq!(
        verified[0].content.kind(),
        Some(yunta_core::ArtifactKind::Tasks)
    );
}

#[test]
fn an_invalid_tasks_document_reports_every_violation_together() {
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

    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap_err();
    // Every violation reaches the reader, not a count of them, and in
    // document order: someone correcting a file works top to bottom.
    assert_eq!(codes(&failures), ["no-criteria", "empty-scope"]);
    let text = rendered(&failures);
    assert!(text.contains("artifacts/plan.yaml: 2 errors"), "{text}");
    assert!(text.contains("task `T001`: no criteria declared"), "{text}");
    assert!(text.contains("task `T002`: `scope` is empty"), "{text}");
}

#[test]
fn a_content_failure_keeps_the_document_every_diagnostic_belongs_to() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "plan.yaml", "tasks: [not, a, document");

    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap_err();
    // Not a flat list of diagnostics: each one is reachable through the
    // document it is about, so a later reader knows which file to open
    // and which kind's rules were asked.
    let report = failures[0].report().expect("a problem with the content");
    assert_eq!(report.document.kind, yunta_core::ArtifactKind::Tasks);
    assert_eq!(report.document.path, "artifacts/plan.yaml");
    assert_eq!(codes(&failures), ["parse"]);
}

#[test]
fn a_problem_with_the_file_itself_has_no_document_to_report_on() {
    let run_dir = tempfile::tempdir().unwrap();

    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), None).unwrap_err();
    // A tasks document that was never written has no content whose kind could
    // be wrong — which is exactly why a rewrite cannot fix it.
    assert!(failures[0].report().is_none(), "{:?}", failures[0]);
    assert_eq!(failures[0].path(), "artifacts/plan.yaml");
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
    let ArtifactContent::Findings(findings) = &verified[0].content else {
        panic!("parsed findings: {:?}", verified[0].content);
    };
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

    let failures = close_artifacts(&node(REVIEW_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&failures), ["duplicate-id", "empty-title"]);
    let text = rendered(&failures);
    assert!(text.contains("artifacts/findings.yaml: 2 errors"), "{text}");
}

#[test]
fn a_malformed_findings_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "findings.yaml", "findings: [not, valid");

    let failures = close_artifacts(&node(REVIEW_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&failures), ["parse"]);
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
    let ArtifactContent::Questions(questions) = &verified[0].content else {
        panic!("parsed questions: {:?}", verified[0].content);
    };
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

    let failures = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&failures).len(), 1);
    let text = rendered(&failures);
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

    let failures = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&failures), ["duplicate-id", "empty-text"]);
    let text = rendered(&failures);
    assert!(
        text.contains("artifacts/questions.yaml: 2 errors"),
        "{text}"
    );
}

#[test]
fn a_malformed_questions_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "questions.yaml", "questions: [not, valid");

    let failures = close_artifacts(&node(ASK_NODE), run_dir.path(), None).unwrap_err();
    assert_eq!(codes(&failures), ["parse"]);
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

    let failures = close_artifacts(&node(REPORT_NODE), run_dir.path(), Some(5)).unwrap_err();
    // Both numbers on the table as data, never a truncation, so whoever
    // reads the log does not have to parse them back out of a sentence.
    match &failures[..] {
        [ArtifactFailure::File {
            problem: FileProblem::Oversized { bytes, ceiling },
            ..
        }] => assert_eq!((*bytes, *ceiling), (10, 5)),
        other => panic!("an oversized file, with both numbers: {other:?}"),
    }
    let text = rendered(&failures);
    assert!(text.contains("artifacts/report.md: 1 error"), "{text}");
    assert!(text.contains("10") && text.contains('5'), "{text}");
}

#[test]
fn an_artifact_at_the_cap_or_with_no_cap_passes() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report.md", "0123456789");

    assert!(close_artifacts(&node(REPORT_NODE), run_dir.path(), Some(10)).is_ok());
    assert!(close_artifacts(&node(REPORT_NODE), run_dir.path(), None).is_ok());
}

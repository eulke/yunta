//! What a node's close makes of everything it declared.
//!
//! Two sources, one verdict. A file a command node wrote is read from
//! that node's staging and enters the run here; a document a session
//! handed over or the engine derived is already a fact of the run's log,
//! and the close reads it back out of the object store without opening
//! anything on disk.

use std::path::Path;

use yunta_core::diagnostic::{ArtifactFailure, FileProblem};
use yunta_core::events::ArtifactEvent;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, EventBody, EventPayload, RecordedOrigin, StoredEvent,
};
use yunta_core::{sha256_hex, ArtifactKind, Node};
use yunta_engine::{close_artifacts, ArtifactContent, ObjectStore};

/// A run that has accepted nothing: what a node with no history closes
/// against.
const NOTHING_HELD: &[StoredEvent] = &[];

/// Every rule broken across every failing artifact, by its stable code —
/// a problem with the file under its own code, a problem with the
/// content under the document's.
fn codes(failures: &[ArtifactFailure]) -> Vec<&str> {
    failures
        .iter()
        .flat_map(|failure| match failure.report() {
            // A problem with the artifact itself is one code; a document
            // whose content failed answers with every problem it has.
            None => failure.code().into_iter().collect::<Vec<_>>(),
            Some(report) => report.diagnostics.iter().map(|d| d.code()).collect(),
        })
        .map(|code| code.as_str())
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

/// Puts a file where `node` writes what it declares, which is where the
/// close reads it back from.
fn write_artifact(run_dir: &Path, node: &str, name: &str, content: &str) {
    let dir = yunta_engine::run_dir::staging(run_dir, &node.into());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), content).unwrap();
}

/// How a diagnostic names one of `node`'s declared files.
fn staged(node: &str, name: &str) -> String {
    format!("scratch/staging/{node}/{name}")
}

/// Takes `content` into the run the way a submission or a derivation
/// does: the bytes in the object store, an `artifact_accepted` under
/// `node` naming their hash.
async fn accepted(run_dir: &Path, node: &str, kind: ArtifactKind, content: &str) -> StoredEvent {
    std::fs::create_dir_all(run_dir.join("scratch")).unwrap();
    let content_hash = ObjectStore::at(run_dir)
        .put(content.as_bytes())
        .await
        .unwrap();
    StoredEvent {
        run_id: "run-1".into(),
        seq: 1.into(),
        timestamp: chrono::Utc::now(),
        node_id: Some(node.into()),
        body: EventBody::Known(EventPayload::Artifacts(ArtifactEvent::Accepted(
            ArtifactAcceptedPayload::new(
                ArtifactId::Interpreted { kind },
                content_hash,
                RecordedOrigin::Submitted,
            ),
        ))),
    }
}

const REPORT_NODE: &str = r#"
id: report
kind: prompt
prompt: "Write the report."
artifacts:
  produces: [report.md]
"#;

/// A command node that writes its tasks document itself — the one kind
/// whose interpreted artifact is a file, and so the one whose close
/// reads the file.
const PLAN_NODE: &str = r#"
id: plan
kind: bash
run: "write the tasks document"
artifacts:
  produces: [tasks]
"#;

/// The same declaration on a session node: the document arrives through
/// the submission tool, so the run's log is the only thing that answers
/// for it.
const PLAN_SESSION_NODE: &str = r#"
id: plan
kind: prompt
prompt: "Write the tasks document."
artifacts:
  produces: [tasks]
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

#[tokio::test]
async fn a_missing_declared_artifact_fails_the_node_no_matter_what_the_agent_said() {
    let run_dir = tempfile::tempdir().unwrap();

    let failures = close_artifacts(&node(REPORT_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    // The node that declared it is on the failure as data, not only in
    // the sentence a reader gets.
    match &failures[..] {
        [ArtifactFailure::File {
            path,
            problem: FileProblem::Missing { node },
        }] => {
            assert_eq!(*path, staged("report", "report.md"));
            assert_eq!(node.as_str(), "report");
        }
        other => panic!("a missing file, named: {other:?}"),
    }
    assert!(rendered(&failures).contains(&staged("report", "report.md")));
}

#[tokio::test]
async fn an_empty_artifact_is_as_bad_as_a_missing_one() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report", "report.md", "");

    let failures = close_artifacts(&node(REPORT_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures), ["artifact-empty"]);
}

#[tokio::test]
async fn every_missing_artifact_is_reported_not_just_the_first() {
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

    let failures = close_artifacts(&n, run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(failures.len(), 2);
}

#[tokio::test]
async fn an_opaque_artifact_is_verified_by_existence_and_hash_never_by_format() {
    let run_dir = tempfile::tempdir().unwrap();
    // Content that is not valid YAML/JSON/anything — the engine
    // assumes no format for opaque artifacts.
    write_artifact(
        run_dir.path(),
        "report",
        "report.md",
        "{{{ not : parseable ][",
    );

    let verified = close_artifacts(&node(REPORT_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(
        verified[0].artifact,
        ArtifactId::Opaque {
            name: "report.md".to_string()
        }
    );
    assert_eq!(
        verified[0]
            .staged
            .as_ref()
            .map(|staged| staged.to_string())
            .as_deref(),
        Some(sha256_hex("{{{ not : parseable ][".as_bytes()).as_str()),
        "the file this node staged, which is what a close read"
    );
    assert_eq!(verified[0].content, ArtifactContent::Opaque);
    assert_eq!(verified[0].content.kind(), None);
}

#[tokio::test]
async fn a_valid_tasks_document_is_parsed_and_returned_for_registration() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "plan", "tasks.yaml", VALID_TASKS);

    let verified = close_artifacts(&node(PLAN_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap();
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

#[tokio::test]
async fn an_invalid_tasks_document_reports_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    // Two independent violations: T001 has no criteria, T002 has an
    // empty scope. Both must surface in one pass.
    write_artifact(
        run_dir.path(),
        "plan",
        "tasks.yaml",
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

    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    // Every violation reaches the reader, not a count of them, and in
    // document order: someone correcting a file works top to bottom.
    assert_eq!(codes(&failures), ["no-criteria", "empty-scope"]);
    let text = rendered(&failures);
    assert!(
        text.contains(&format!("{}: 2 errors", staged("plan", "tasks.yaml"))),
        "{text}"
    );
    assert!(text.contains("task `T001`: no criteria declared"), "{text}");
    assert!(text.contains("task `T002`: `scope` is empty"), "{text}");
}

#[tokio::test]
async fn a_content_failure_keeps_the_document_every_diagnostic_belongs_to() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "plan",
        "tasks.yaml",
        "tasks: [not, a, document",
    );

    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    // Not a flat list of diagnostics: each one is reachable through the
    // document it is about, so a later reader knows which file to open
    // and which kind's rules were asked.
    let report = failures[0].report().expect("a problem with the content");
    assert_eq!(report.document.kind, yunta_core::ArtifactKind::Tasks);
    assert_eq!(report.document.path, staged("plan", "tasks.yaml"));
    assert_eq!(codes(&failures), ["parse"]);
}

#[tokio::test]
async fn a_problem_with_the_file_itself_has_no_document_to_report_on() {
    let run_dir = tempfile::tempdir().unwrap();

    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    // A tasks document that was never written has no content whose kind could
    // be wrong — which is exactly why a rewrite cannot fix it.
    assert!(failures[0].report().is_none(), "{:?}", failures[0]);
    assert_eq!(
        failures[0].path(),
        Some(staged("plan", "tasks.yaml").as_str())
    );
}

// --- an interpreted artifact the run already holds ----------------------------

#[tokio::test]
async fn a_document_the_run_holds_closes_its_node_with_no_file_anywhere() {
    let run_dir = tempfile::tempdir().unwrap();
    // Nothing was ever written where a node writes: the run's own
    // acceptance is the whole answer, and the bytes come from the store.
    let held = [accepted(run_dir.path(), "plan", ArtifactKind::Tasks, VALID_TASKS).await];

    let verified = close_artifacts(&node(PLAN_SESSION_NODE), run_dir.path(), &held, None)
        .await
        .unwrap();
    let ArtifactContent::Tasks(tasks) = &verified[0].content else {
        panic!("a parsed tasks document: {:?}", verified[0].content);
    };
    let ids: Vec<&str> = tasks.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["T001", "T002"]);
    assert!(
        !run_dir.path().join("scratch").join("staging").exists(),
        "the close opens no staging directory for a document the run holds"
    );
}

#[tokio::test]
async fn a_session_node_s_document_is_the_one_it_handed_over_never_a_file_beside_it() {
    let run_dir = tempfile::tempdir().unwrap();
    let held = [accepted(run_dir.path(), "plan", ArtifactKind::Tasks, VALID_TASKS).await];
    // A file of the declared name, with a different document in it. The
    // run accepted the other one, and a file nobody accepted is not an
    // artifact of the run.
    write_artifact(
        run_dir.path(),
        "plan",
        "tasks.yaml",
        r#"
tasks:
  - id: IMPOSTOR
    title: "Never handed over"
    scope: ["src/x/"]
    criteria:
      - cmd: "test -f src/x/done"
"#,
    );

    let verified = close_artifacts(&node(PLAN_SESSION_NODE), run_dir.path(), &held, None)
        .await
        .unwrap();
    let ArtifactContent::Tasks(tasks) = &verified[0].content else {
        panic!("a parsed tasks document: {:?}", verified[0].content);
    };
    let ids: Vec<&str> = tasks.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["T001", "T002"]);
}

#[tokio::test]
async fn a_session_node_that_handed_nothing_over_owes_the_document_it_declared() {
    let run_dir = tempfile::tempdir().unwrap();
    // A valid document sits exactly where a command node would write
    // one. This node is not a command node: nobody handed the document
    // over, and nothing on disk changes that.
    write_artifact(run_dir.path(), "plan", "tasks.yaml", VALID_TASKS);

    let failures = close_artifacts(&node(PLAN_SESSION_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    match &failures[..] {
        [ArtifactFailure::Undelivered { node, artifact }] => {
            assert_eq!(node.as_str(), "plan");
            assert_eq!(
                *artifact,
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Tasks
                }
            );
        }
        other => panic!("a document nobody handed over: {other:?}"),
    }
    assert_eq!(codes(&failures), ["artifact-undelivered"]);
    // No file is at fault, so none is named: the close never opened one
    // and a path would send a reader to something that was never going
    // to be there.
    assert_eq!(failures[0].path(), None);
    let rendered = rendered(&failures);
    assert!(
        rendered.contains("node `plan`") && rendered.contains("tasks document"),
        "the failure names the node and the identity it owes: {rendered}"
    );
    assert!(
        !rendered.contains("scratch/") && !rendered.contains("artifacts/"),
        "a document nobody handed over names no file: {rendered}"
    );
}

#[tokio::test]
async fn a_command_node_that_wrote_no_file_still_names_the_file_it_did_not_write() {
    let run_dir = tempfile::tempdir().unwrap();

    // The same tasks document, declared by a node whose close does open
    // a file: the path is real, the close went looking for it, and that
    // is what the failure says.
    let failures = close_artifacts(&node(PLAN_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    match &failures[..] {
        [ArtifactFailure::File {
            path,
            problem: FileProblem::Missing { node },
        }] => {
            assert_eq!(*path, staged("plan", "tasks.yaml"));
            assert_eq!(node.as_str(), "plan");
        }
        other => panic!("a file the node never wrote: {other:?}"),
    }
    assert_eq!(codes(&failures), ["artifact-missing"]);
}

#[tokio::test]
async fn a_held_document_whose_bytes_the_store_lost_says_so_instead_of_reading_a_file() {
    let run_dir = tempfile::tempdir().unwrap();
    let held = [accepted(run_dir.path(), "plan", ArtifactKind::Tasks, VALID_TASKS).await];
    std::fs::remove_dir_all(run_dir.path().join("objects")).unwrap();
    write_artifact(run_dir.path(), "plan", "tasks.yaml", VALID_TASKS);

    let failures = close_artifacts(&node(PLAN_SESSION_NODE), run_dir.path(), &held, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures), ["artifact-unreadable"]);
    assert!(
        rendered(&failures).contains("artifacts/plan/tasks.yaml"),
        "the failure names the artifact by the view a reader opens: {}",
        rendered(&failures)
    );
}

const REVIEW_NODE: &str = r#"
id: review
kind: bash
run: "write the findings"
artifacts:
  produces: [findings]
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

#[tokio::test]
async fn a_valid_findings_artifact_is_parsed_and_returned() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "review", "findings.yaml", VALID_FINDINGS);

    let verified = close_artifacts(&node(REVIEW_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap();
    let ArtifactContent::Findings(findings) = &verified[0].content else {
        panic!("parsed findings: {:?}", verified[0].content);
    };
    let ids: Vec<&str> = findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["f1", "f2"]);
}

#[tokio::test]
async fn duplicate_finding_ids_report_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "review",
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

    let failures = close_artifacts(&node(REVIEW_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures), ["duplicate-id", "empty-title"]);
    let text = rendered(&failures);
    assert!(
        text.contains(&format!("{}: 2 errors", staged("review", "findings.yaml"))),
        "{text}"
    );
}

#[tokio::test]
async fn a_malformed_findings_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "review",
        "findings.yaml",
        "findings: [not, valid",
    );

    let failures = close_artifacts(&node(REVIEW_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures), ["parse"]);
}

const ASK_NODE: &str = r#"
id: ask
kind: bash
run: "write the questions"
artifacts:
  produces: [questions]
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

#[tokio::test]
async fn a_valid_questions_artifact_is_parsed_and_returned() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "ask", "questions.yaml", VALID_QUESTIONS);

    let verified = close_artifacts(&node(ASK_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap();
    let ArtifactContent::Questions(questions) = &verified[0].content else {
        panic!("parsed questions: {:?}", verified[0].content);
    };
    let ids: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["q1", "q2"]);
}

#[tokio::test]
async fn a_choice_question_with_no_values_is_a_reported_violation() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "ask",
        "questions.yaml",
        r#"
questions:
  - id: q1
    text: "Which environment?"
    answer_type: choice
    required: true
"#,
    );

    let failures = close_artifacts(&node(ASK_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures).len(), 1);
    let text = rendered(&failures);
    assert!(text.contains(&staged("ask", "questions.yaml")), "{text}");
}

#[tokio::test]
async fn duplicate_question_ids_report_every_violation_together() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "ask",
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

    let failures = close_artifacts(&node(ASK_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures), ["duplicate-id", "empty-text"]);
    let text = rendered(&failures);
    assert!(
        text.contains(&format!("{}: 2 errors", staged("ask", "questions.yaml"))),
        "{text}"
    );
}

#[tokio::test]
async fn a_malformed_questions_yaml_is_a_typed_error_not_a_panic() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(
        run_dir.path(),
        "ask",
        "questions.yaml",
        "questions: [not, valid",
    );

    let failures = close_artifacts(&node(ASK_NODE), run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap_err();
    assert_eq!(codes(&failures), ["parse"]);
}

#[tokio::test]
async fn a_node_that_declares_no_artifacts_verifies_nothing() {
    let run_dir = tempfile::tempdir().unwrap();
    let n = node(
        r#"
id: quiet
kind: bash
run: "true"
"#,
    );

    let verified = close_artifacts(&n, run_dir.path(), NOTHING_HELD, None)
        .await
        .unwrap();
    assert!(verified.is_empty());
}

// --- limits.max_artifact_bytes ------------------------------------------------

#[tokio::test]
async fn an_artifact_over_max_artifact_bytes_fails_the_node_with_the_sizes_named() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report", "report.md", "0123456789");

    let failures = close_artifacts(&node(REPORT_NODE), run_dir.path(), NOTHING_HELD, Some(5))
        .await
        .unwrap_err();
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
    assert!(
        text.contains(&format!("{}: 1 error", staged("report", "report.md"))),
        "{text}"
    );
    assert!(text.contains("10") && text.contains('5'), "{text}");
}

#[tokio::test]
async fn an_artifact_at_the_cap_or_with_no_cap_passes() {
    let run_dir = tempfile::tempdir().unwrap();
    write_artifact(run_dir.path(), "report", "report.md", "0123456789");

    assert!(
        close_artifacts(&node(REPORT_NODE), run_dir.path(), NOTHING_HELD, Some(10))
            .await
            .is_ok()
    );
    assert!(
        close_artifacts(&node(REPORT_NODE), run_dir.path(), NOTHING_HELD, None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_rendered_artifact_name_that_leaves_the_run_dir_fails_the_node() {
    let bench = yunta_testkit::Bench::new();

    // The name is a template, so what `check` reads is `{{...}}` and
    // what the view would be given is whatever it stands for — here an
    // absolute path, which would write the file outside the run
    // entirely.
    let workflow = r#"
name: escaping-name
nodes:
  - id: write
    kind: bash
    run: "true"
    artifacts:
      produces: ["{{node.artifacts}}/report.md"]
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []\n").await;

    assert!(matches!(terminal, yunta_engine::RunTerminal::Paused { .. }));
    match state.nodes.state("write") {
        Some(yunta_engine::NodeState::Failed { failure, .. }) => {
            let said = failure.to_string();
            assert!(
                said.contains("reaches outside the run directory"),
                "the failure names the rendered name and the rule, got {said}",
            );
        }
        other => panic!("a name that leaves the run directory fails its node, got {other:?}"),
    }
}

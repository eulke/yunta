//! A diagnostic names what is wrong in the vocabulary of the document,
//! and a report renders every one of them the way `spec-ledger.md` §4
//! fixes it. Nothing a deserializer says about its own internals
//! reaches either rendering.

use yunta_core::diagnostic::{
    ArtifactFailure, Diagnostic, DiagnosticCode, DocumentRef, FileProblem, Named, Problem, Report,
    RuleCode, Subject,
};
use yunta_core::events::{ArtifactId, Failure};
use yunta_core::{ArtifactKind, NodeId, RunId, TaskId};

fn plan() -> DocumentRef {
    DocumentRef::new(ArtifactKind::Tasks, "artifacts/plan.yaml")
}

fn task(id: &str, index: usize) -> Subject {
    Subject::Task(Named::new(TaskId::from(id), index))
}

fn broke(code: RuleCode, detail: &str) -> Diagnostic {
    Diagnostic::new(task("t1", 0), Problem::rule(code, detail))
}

#[test]
fn a_named_task_renders_by_its_id_never_by_its_position() {
    assert_eq!(task("t1", 0).to_string(), "task `t1`");
}

#[test]
fn a_task_whose_id_did_not_parse_renders_by_its_ordinal() {
    assert_eq!(
        Subject::Task(Named::new(None, 0)).to_string(),
        "the first task"
    );
    assert_eq!(
        Subject::Task(Named::new(None, 4)).to_string(),
        "the 5th task"
    );
}

#[test]
fn a_criterion_names_the_task_it_belongs_to() {
    let subject = Subject::Criterion {
        task: Named::new(TaskId::from("t1"), 0),
        index: 0,
    };
    assert_eq!(subject.to_string(), "task `t1`, criterion 1");

    // And when the task's own id is what could not be read, the
    // criterion still says which task it belongs to.
    let subject = Subject::Criterion {
        task: Named::new(None, 0),
        index: 1,
    };
    assert_eq!(subject.to_string(), "the first task, criterion 2");
}

#[test]
fn a_criterion_carries_its_task_s_position_as_one_value_with_its_id() {
    // The id and the position are one concept, so a diagnostic that
    // round-trips cannot come back with the id and lose the position —
    // which would silently rename the task it blames.
    let subject = Subject::Criterion {
        task: Named::new(None, 3),
        index: 0,
    };
    let json = serde_json::to_string(&subject).expect("a subject serializes");
    let back: Subject = serde_json::from_str(&json).expect("and reads back identical");
    assert_eq!(back, subject);
    assert_eq!(back.to_string(), "the 4th task, criterion 1");
}

#[test]
fn a_report_for_a_person_lists_every_violation_on_its_own_line() {
    let report = Report::new(
        plan(),
        vec![
            broke(
                RuleCode::EmptyScope,
                "`scope` is empty; every task must declare at least one glob",
            ),
            broke(
                RuleCode::NoCriteria,
                "no criteria declared; every task needs at least one",
            ),
        ],
    );

    assert_eq!(
        report.to_string(),
        "artifacts/plan.yaml: 2 errors\n  \
         task `t1`: `scope` is empty; every task must declare at least one glob\n  \
         task `t1`: no criteria declared; every task needs at least one"
    );
}

#[test]
fn one_violation_is_reported_in_the_singular() {
    let report = Report::new(plan(), vec![broke(RuleCode::NoCriteria, "no criteria")]);
    assert!(report
        .to_string()
        .starts_with("artifacts/plan.yaml: 1 error\n"));
}

#[test]
fn a_diagnostic_survives_the_event_log_as_data() {
    let diagnostic = broke(RuleCode::EmptyScope, "`scope` is empty");
    let json = serde_json::to_string(&diagnostic).expect("a diagnostic serializes");
    let back: Diagnostic = serde_json::from_str(&json).expect("and reads back identical");
    assert_eq!(back, diagnostic);
    assert_eq!(back.code().as_str(), "empty-scope");
}

#[test]
fn every_diagnostic_has_a_stable_code_for_counting() {
    let cases = [
        (Problem::parse("tasks[0].id", "invalid type"), "parse"),
        (
            Problem::rule(RuleCode::DependencyCycle, ""),
            "dependency-cycle",
        ),
    ];
    for (problem, expected) in cases {
        assert_eq!(
            Diagnostic::new(Subject::Document, problem).code().as_str(),
            expected
        );
    }
}

// --- a failure is data, and the prose comes from it --------------------

#[test]
fn a_file_that_was_never_written_is_a_different_failure_from_one_written_wrong() {
    let missing = ArtifactFailure::file(
        "artifacts/plan.yaml",
        FileProblem::Missing {
            node: NodeId::from("plan"),
        },
    );
    let malformed = ArtifactFailure::Content(Report::new(
        plan(),
        vec![broke(RuleCode::NoCriteria, "no criteria declared")],
    ));

    assert!(
        missing.report().is_none(),
        "a file nobody wrote names no document"
    );
    assert!(
        malformed.report().is_some(),
        "a document that did not read names itself"
    );
    assert_eq!(missing.path(), Some("artifacts/plan.yaml"));
    assert!(missing.report().is_none());
    assert!(malformed.report().is_some());
}

#[test]
fn a_failure_keeps_the_document_every_problem_came_from() {
    let findings = DocumentRef::new(ArtifactKind::Findings, "artifacts/findings.yaml");
    let failure = Failure::artifacts(vec![
        ArtifactFailure::Content(Report::new(
            plan(),
            vec![broke(RuleCode::NoCriteria, "no criteria declared")],
        )),
        ArtifactFailure::Content(Report::new(
            findings,
            vec![Diagnostic::new(
                Subject::Finding(Named::new(None, 0)),
                Problem::rule(RuleCode::EmptyDetail, "`detail` is empty"),
            )],
        )),
    ]);

    let attributed: Vec<(&str, Vec<&str>)> = failure
        .reports()
        .map(|report| {
            (
                report.document.path.as_str(),
                report
                    .diagnostics
                    .iter()
                    .map(|d| d.code().as_str())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        attributed,
        vec![
            ("artifacts/plan.yaml", vec!["no-criteria"]),
            ("artifacts/findings.yaml", vec!["empty-detail"]),
        ],
        "a reader can say which file each problem came from"
    );
}

#[test]
fn a_failure_renders_one_block_per_failing_document() {
    let failure = Failure::artifacts(vec![
        ArtifactFailure::file(
            "artifacts/notes.md",
            FileProblem::Missing {
                node: NodeId::from("plan"),
            },
        ),
        ArtifactFailure::Content(Report::new(
            plan(),
            vec![broke(RuleCode::NoCriteria, "no criteria declared")],
        )),
    ]);
    assert_eq!(
        failure.to_string(),
        "artifacts/notes.md: 1 error\n  \
         the document was declared by node `plan` and never produced\n\
         artifacts/plan.yaml: 1 error\n  \
         task `t1`: no criteria declared"
    );
}

#[test]
fn a_failure_stated_in_one_sentence_renders_as_that_sentence() {
    let failure = Failure::message("the runner exited with status 2");
    assert_eq!(failure.to_string(), "the runner exited with status 2");
    assert_eq!(failure.reports().count(), 0);
}

#[test]
fn a_log_written_before_failures_were_data_still_reads() {
    // The tolerance rule for what is persisted: `outcome` alone is a
    // message, and nothing about the older line has to be rewritten.
    let older: Failure =
        serde_json::from_str(r#"{"outcome":"the runner exited with status 2"}"#).expect("reads");
    assert_eq!(older, Failure::message("the runner exited with status 2"));
}

#[test]
fn a_document_nobody_handed_over_is_a_failure_of_the_artifact_itself() {
    let undelivered = ArtifactFailure::Undelivered {
        node: NodeId::from("plan"),
        artifact: ArtifactId::of("plan.yaml", Some(ArtifactKind::Tasks)),
    };

    assert_eq!(
        undelivered.code().map(DiagnosticCode::as_str),
        Some("artifact-undelivered"),
        "a receipt counts it under its own stable name"
    );
    assert!(
        undelivered.path().is_none(),
        "the close opened no file, so there is none to open"
    );
    assert!(
        undelivered.report().is_none(),
        "nothing read a document that was never handed over"
    );
}

#[test]
fn an_undelivered_document_reads_back_with_its_tag_the_node_and_the_identity() {
    let undelivered = ArtifactFailure::Undelivered {
        node: NodeId::from("plan"),
        artifact: ArtifactId::of("plan.yaml", Some(ArtifactKind::Tasks)),
    };

    let value = serde_json::to_value(&undelivered).expect("a failure is data");
    assert_eq!(value["failure"], "undelivered", "{value:#}");
    assert_eq!(value["node"], "plan", "{value:#}");
    assert_eq!(value["artifact"]["kind"], "tasks", "{value:#}");
    assert!(
        value.get("path").is_none(),
        "no file is named, not even an empty one: {value:#}"
    );

    let back: ArtifactFailure = serde_json::from_value(value).expect("and reads back as itself");
    assert_eq!(back, undelivered);
}

#[test]
fn a_document_a_node_never_handed_over_renders_as_the_node_that_owes_it() {
    let failure = Failure::artifacts(vec![ArtifactFailure::Undelivered {
        node: NodeId::from("plan"),
        artifact: ArtifactId::of("plan.yaml", Some(ArtifactKind::Tasks)),
    }]);
    assert_eq!(
        failure.to_string(),
        "node `plan`: 1 error\n  \
         handed over no tasks document — produce it before the node ends, \
         or stop declaring it here"
    );
    assert_eq!(
        failure.failures().count(),
        1,
        "it is one declared artifact that did not close"
    );
    assert_eq!(failure.reports().count(), 0, "no document was read");
}

#[test]
fn an_artifact_no_run_holds_is_a_failure_of_the_artifact_itself() {
    let unheld = ArtifactFailure::Unheld {
        run: RunId::from("run-child-1"),
        producer: None,
        artifact: ArtifactId::of("report.md", None),
    };

    assert_eq!(
        unheld.code().map(DiagnosticCode::as_str),
        Some("artifact-unheld"),
        "a receipt counts it under its own stable name"
    );
    assert!(
        unheld.path().is_none(),
        "no node wrote a file, so there is none to open"
    );
    assert!(
        unheld.report().is_none(),
        "nothing read a document that was never handed over"
    );
}

#[test]
fn an_unheld_artifact_reads_back_with_its_tag_and_the_run_it_was_missing_from() {
    let unheld = ArtifactFailure::Unheld {
        run: RunId::from("run-child-1"),
        producer: Some(NodeId::from("plan")),
        artifact: ArtifactId::of("plan.yaml", Some(ArtifactKind::Tasks)),
    };

    let value = serde_json::to_value(&unheld).expect("a failure is data");
    assert_eq!(value["failure"], "unheld", "{value:#}");
    assert_eq!(value["run"], "run-child-1", "{value:#}");
    assert_eq!(value["producer"], "plan", "{value:#}");
    assert_eq!(value["artifact"]["kind"], "tasks", "{value:#}");

    let back: ArtifactFailure = serde_json::from_value(value).expect("and reads back as itself");
    assert_eq!(back, unheld);
}

#[test]
fn an_artifact_another_run_owes_renders_as_the_run_that_does_not_hold_it() {
    let failure = Failure::artifacts(vec![ArtifactFailure::Unheld {
        run: RunId::from("run-child-1"),
        producer: None,
        artifact: ArtifactId::of("report.md", None),
    }]);
    assert_eq!(
        failure.to_string(),
        "run `run-child-1`: 1 error\n  \
         holds no artifact `report.md` — produce it there, or stop declaring it here"
    );
    assert_eq!(
        failure.failures().count(),
        1,
        "it is one declared artifact that did not close"
    );
    assert_eq!(failure.reports().count(), 0, "no document was read");
}

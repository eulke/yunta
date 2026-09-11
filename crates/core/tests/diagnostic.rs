//! A diagnostic names what is wrong in the vocabulary of the document,
//! and a report renders every one of them the way `spec-ledger.md` §4
//! fixes it. Nothing a deserializer says about its own internals
//! reaches either rendering.

use yunta_core::diagnostic::{
    ArtifactFailure, Diagnostic, DocumentRef, FileProblem, Malformation, Named, Problem, Report,
    RuleCode, Subject, ValueShape,
};
use yunta_core::events::Failure;
use yunta_core::{ArtifactKind, NodeId, TaskId};

fn plan() -> DocumentRef {
    DocumentRef::new(ArtifactKind::TaskLedger, "artifacts/plan.yaml")
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
fn an_unknown_key_lists_the_keys_that_are_accepted() {
    let diagnostic = Diagnostic::new(
        task("t1", 0),
        Problem::unknown_key("description", ["id", "title", "scope", "criteria"]),
    );
    assert_eq!(
        diagnostic.to_string(),
        "task `t1`: unknown key `description`; a task declares `id`, `title`, `scope`, `criteria`"
    );
}

#[test]
fn a_retired_key_carries_the_key_that_replaced_it() {
    let diagnostic = Diagnostic::new(
        Subject::Document,
        Problem::unknown_key_instead("role", ["runner"], "a node names its runner with `runner:`"),
    );
    assert!(diagnostic
        .to_string()
        .ends_with("a node names its runner with `runner:`"));
}

#[test]
fn a_wrong_shape_shows_what_was_written_and_what_to_write() {
    let diagnostic = Diagnostic::new(
        Subject::Criterion {
            task: Named::new(TaskId::from("t1"), 0),
            index: 0,
        },
        Problem::wrong_shape(ValueShape::String, "a mapping", "- cmd: \"cargo test\""),
    );
    let text = diagnostic.to_string();
    assert!(text.contains("task `t1`, criterion 1"), "{text}");
    assert!(text.contains("a mapping"), "{text}");
    assert!(text.contains("a string"), "{text}");
    assert!(text.contains("- cmd: \"cargo test\""), "{text}");
}

#[test]
fn a_markdown_fence_is_named_as_such_not_as_a_stray_character() {
    let diagnostic = Diagnostic::new(
        Subject::Document,
        Problem::not_yaml(
            Some(Malformation::MarkdownFence),
            "found character that cannot start any token",
        ),
    );
    let text = diagnostic.to_string();
    assert!(text.contains("Markdown code fence"), "{text}");
    assert!(
        !text.contains("cannot start any token"),
        "the parser's own words never reach a reader: {text}"
    );
}

#[test]
fn a_diagnostic_survives_the_event_log_as_data() {
    let diagnostic = broke(RuleCode::EmptyScope, "`scope` is empty");
    let json = serde_json::to_string(&diagnostic).expect("a diagnostic serializes");
    let back: Diagnostic = serde_json::from_str(&json).expect("and reads back identical");
    assert_eq!(back, diagnostic);
    assert_eq!(back.code(), "empty-scope");
}

#[test]
fn every_diagnostic_has_a_stable_code_for_counting() {
    let cases = [
        (Problem::not_yaml(None, ""), "not-yaml"),
        (
            Problem::unknown_key("x", Vec::<String>::new()),
            "unknown-key",
        ),
        (Problem::missing_key("id"), "missing-key"),
        (
            Problem::wrong_shape(ValueShape::Null, "a mapping", "id: x"),
            "wrong-shape",
        ),
        (Problem::invalid_id("1", "a letter first"), "invalid-id"),
        (
            Problem::rule(RuleCode::DependencyCycle, ""),
            "dependency-cycle",
        ),
    ];
    for (problem, expected) in cases {
        assert_eq!(Diagnostic::new(Subject::Document, problem).code(), expected);
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

    assert!(!missing.is_repairable(), "nothing in it to correct");
    assert!(malformed.is_repairable(), "writing it again fixes it");
    assert_eq!(missing.path(), "artifacts/plan.yaml");
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
                report.diagnostics.iter().map(Diagnostic::code).collect(),
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

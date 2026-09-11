//! A diagnostic names what is wrong in the vocabulary of the document,
//! and a report renders every one of them the way `spec-ledger.md` §4
//! fixes it. Nothing a deserializer says about its own internals
//! reaches either rendering.

use yunta_core::diagnostic::{
    Diagnostic, DocumentKind, DocumentRef, Malformation, Problem, Report, Subject, ValueShape,
};
use yunta_core::TaskId;

/// Stands in for the shape a caller hands `for_agent`; the real one
/// comes from the type that parses the document.
const LEDGER_SHAPE: &str = "tasks:\n  - id: add-dark-mode\n";

fn plan() -> DocumentRef {
    DocumentRef::new(DocumentKind::TaskLedger, "artifacts/plan.yaml")
}

fn task(id: &str, index: usize) -> Subject {
    Subject::Task {
        id: Some(TaskId::from(id)),
        index,
    }
}

#[test]
fn a_named_task_renders_by_its_id_never_by_its_position() {
    let subject = task("t1", 0);
    assert_eq!(subject.to_string(), "task `t1`");
}

#[test]
fn a_task_whose_id_did_not_parse_renders_by_its_ordinal() {
    let subject = Subject::Task { id: None, index: 0 };
    assert_eq!(subject.to_string(), "the first task");

    let subject = Subject::Task { id: None, index: 4 };
    assert_eq!(subject.to_string(), "the 5th task");
}

#[test]
fn a_criterion_names_the_task_it_belongs_to() {
    let subject = Subject::Criterion {
        task: Some(TaskId::from("t1")),
        index: 0,
    };
    assert_eq!(subject.to_string(), "task `t1`, criterion 1");
}

#[test]
fn a_report_for_a_person_lists_every_violation_on_its_own_line() {
    let report = Report::new(
        plan(),
        vec![
            Diagnostic::new(
                task("t1", 0),
                Problem::rule(
                    "empty-scope",
                    "`scope` is empty; every task must declare at least one glob",
                ),
            ),
            Diagnostic::new(
                task("t1", 0),
                Problem::rule(
                    "no-criteria",
                    "no criteria declared; every task needs at least one",
                ),
            ),
        ],
    );

    assert_eq!(
        report.for_person(),
        "artifacts/plan.yaml: 2 errors\n  \
         task `t1`: `scope` is empty; every task must declare at least one glob\n  \
         task `t1`: no criteria declared; every task needs at least one"
    );
}

#[test]
fn one_violation_is_reported_in_the_singular() {
    let report = Report::new(
        plan(),
        vec![Diagnostic::new(
            task("t1", 0),
            Problem::rule(
                "no-criteria",
                "no criteria declared; every task needs at least one",
            ),
        )],
    );
    assert!(report
        .for_person()
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
            task: Some(TaskId::from("t1")),
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
fn a_report_for_an_agent_is_an_instruction_to_rewrite_the_file() {
    let report = Report::new(
        plan(),
        vec![Diagnostic::new(
            task("t1", 0),
            Problem::rule(
                "no-criteria",
                "no criteria declared; every task needs at least one",
            ),
        )],
    );
    let text = report.for_agent(Some(LEDGER_SHAPE));
    assert!(text.contains("artifacts/plan.yaml"), "{text}");
    assert!(text.contains("could not be read"), "{text}");
    assert!(text.contains("write the file again"), "{text}");
    assert!(text.contains("1. task `t1`"), "{text}");
}

#[test]
fn a_report_for_an_agent_carries_the_shape_it_should_have_written() {
    let report = Report::new(
        plan(),
        vec![Diagnostic::new(
            task("t1", 0),
            Problem::rule("no-criteria", "no criteria declared"),
        )],
    );
    let text = report.for_agent(Some(LEDGER_SHAPE));
    assert!(
        text.contains("tasks:"),
        "the agent that never saw the shape gets it here: {text}"
    );
}

#[test]
fn a_diagnostic_survives_the_event_log_as_data() {
    let diagnostic = Diagnostic::new(
        task("t1", 0),
        Problem::rule("empty-scope", "`scope` is empty"),
    );
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
    ];
    for (problem, expected) in cases {
        assert_eq!(Diagnostic::new(Subject::Document, problem).code(), expected);
    }
}

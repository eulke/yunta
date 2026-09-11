//! What each interpreted document is, in the vocabulary its own readers
//! use: which keys it declares, which a writer plausibly reaches for
//! instead, and the values its closed sets accept.
//!
//! Kept apart from the frontier that reads it and the helpers it walks
//! with, because this is the only part that changes when a schema does.

use crate::diagnostic::{Diagnostic, Problem, Subject};
use crate::events::FindingSeverity;
use crate::yaml::{Mapping, Value};
use crate::{FindingId, FindingsFile, Ledger, QuestionId, QuestionsFile, TaskId};

use super::walk::{
    as_mapping, as_sequence, check_keys, check_string, check_string_list, read_id, shape_of,
    walk_entries, Hint,
};
use super::{Shaped, FINDINGS_EXAMPLE, LEDGER_EXAMPLE, QUESTIONS_EXAMPLE};
use crate::diagnostic::DocumentKind;

// --- task ledger ---------------------------------------------------------

const TASK_REQUIRED: &[&str] = &["id", "title", "scope", "criteria"];
const TASK_OPTIONAL: &[&str] = &["depends_on", "notes", "manual_review", "justification"];
/// Keys a writer reaches for that this schema spells differently.
const TASK_HINTS: &[Hint] = &[
    ("description", "notes", "a task's prose goes in `notes:`"),
    (
        "acceptance_criteria",
        "criteria",
        "verification commands go in `criteria:`",
    ),
    (
        "files",
        "scope",
        "the paths a task may touch are its `scope:`",
    ),
    (
        "dependencies",
        "depends_on",
        "a task names its prerequisites with `depends_on:`",
    ),
    (
        "status",
        "",
        "task status lives in the event log, never in the ledger",
    ),
    (
        "done",
        "",
        "task status lives in the event log, never in the ledger",
    ),
];

impl Shaped for Ledger {
    const KIND: DocumentKind = DocumentKind::TaskLedger;
    const EXAMPLE: &'static str = LEDGER_EXAMPLE;

    fn diagnose(value: &Value, into: &mut Vec<Diagnostic>) {
        walk_entries(value, "tasks", LEDGER_EXAMPLE, into, diagnose_task);
    }
}

/// One task: named by its own id first, so every problem below carries
/// it, and by position when the id is what could not be read.
fn diagnose_task(index: usize, item: &Value, into: &mut Vec<Diagnostic>) {
    {
        let mut subject = Subject::Task { id: None, index };
        let Some(map) = as_mapping(item, &subject, "a mapping", "- id: add-dark-mode", into) else {
            return;
        };
        if let Some(id) = read_id::<TaskId>(map, &subject, into) {
            subject = Subject::Task {
                id: Some(id),
                index,
            };
        }
        check_keys(
            map,
            &subject,
            TASK_REQUIRED,
            TASK_OPTIONAL,
            TASK_HINTS,
            into,
        );
        check_string(map, "title", &subject, "title: \"Add the toggle\"", into);
        check_string(
            map,
            "notes",
            &subject,
            "notes: \"One line of context.\"",
            into,
        );
        check_string_list(
            map,
            "scope",
            &subject,
            "a list of globs",
            "scope: [\"src/**\"]",
            into,
        );
        check_string_list(
            map,
            "depends_on",
            &subject,
            "a list of task ids",
            "depends_on: [add-dark-mode]",
            into,
        );
        let task_id = match &subject {
            Subject::Task { id, .. } => id.clone(),
            _ => None,
        };
        walk_criteria(map, task_id, into);
    }
}

fn walk_criteria(task: &Mapping, task_id: Option<TaskId>, into: &mut Vec<Diagnostic>) {
    let Some(value) = task.get("criteria") else {
        return;
    };
    let owner = Subject::Task {
        id: task_id.clone(),
        index: 0,
    };
    let Some(items) = as_sequence(
        value,
        &owner,
        "a list of criteria",
        "criteria:\n      - cmd: \"cargo test\"",
        into,
    ) else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        let subject = Subject::Criterion {
            task: task_id.clone(),
            index,
        };
        let Some(map) = as_mapping(item, &subject, "a mapping", "- cmd: \"cargo test\"", into)
        else {
            continue;
        };
        check_keys(map, &subject, &["cmd"], &["type"], &[], into);
        check_string(map, "cmd", &subject, "cmd: \"cargo test\"", into);
        if let Some(kind) = map.get("type") {
            let valid = ["guard"];
            match kind.as_str() {
                Some("guard") => {}
                Some(other) => into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::unknown_value(other, valid),
                )),
                None => into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::wrong_shape(shape_of(kind), "text", "type: guard"),
                )),
            }
        }
    }
}

// --- findings ------------------------------------------------------------

const FINDING_REQUIRED: &[&str] = &["id", "severity", "title", "location", "detail"];
const FINDING_OPTIONAL: &[&str] = &["proposed_criterion"];
const FINDING_HINTS: &[Hint] = &[
    (
        "description",
        "detail",
        "the body of a finding is `detail:`",
    ),
    ("message", "title", "the one-line summary is `title:`"),
    (
        "file",
        "location",
        "the path and its range go together in `location:`",
    ),
    (
        "line",
        "location",
        "the path and its range go together in `location:`",
    ),
    (
        "path",
        "location",
        "the path and its range go together in `location:`",
    ),
    (
        "severity_level",
        "severity",
        "the ladder is declared with `severity:`",
    ),
];

impl Shaped for FindingsFile {
    const KIND: DocumentKind = DocumentKind::Findings;
    const EXAMPLE: &'static str = FINDINGS_EXAMPLE;

    fn diagnose(value: &Value, into: &mut Vec<Diagnostic>) {
        walk_entries(value, "findings", FINDINGS_EXAMPLE, into, diagnose_finding);
    }
}

/// One finding, named by its own id when that parsed.
fn diagnose_finding(index: usize, item: &Value, into: &mut Vec<Diagnostic>) {
    {
        let mut subject = Subject::Finding { id: None, index };
        let Some(map) = as_mapping(
            item,
            &subject,
            "a mapping",
            "- id: null-deref-on-resize",
            into,
        ) else {
            return;
        };
        if let Some(id) = read_id::<FindingId>(map, &subject, into) {
            subject = Subject::Finding {
                id: Some(id),
                index,
            };
        }
        check_keys(
            map,
            &subject,
            FINDING_REQUIRED,
            FINDING_OPTIONAL,
            FINDING_HINTS,
            into,
        );
        for (key, example) in [
            ("title", "title: \"Resize handler dereferences null\""),
            ("location", "location: \"src/ui/resize.rs:142-150\""),
            ("detail", "detail: \"What goes wrong, and when.\""),
        ] {
            check_string(map, key, &subject, example, into);
        }
        check_severity(map, &subject, into);
        if let Some(criterion) = map.get("proposed_criterion") {
            if let Some(inner) = as_mapping(
                criterion,
                &subject,
                "a mapping",
                "proposed_criterion: { cmd: \"cargo test resize\" }",
                into,
            ) {
                check_keys(inner, &subject, &["cmd"], &[], &[], into);
            }
        }
    }
}

fn check_severity(map: &Mapping, subject: &Subject, into: &mut Vec<Diagnostic>) {
    const LADDER: [&str; 4] = ["blocking", "major", "minor", "note"];
    let Some(value) = map.get("severity") else {
        return;
    };
    match value.as_str() {
        Some(text) if crate::yaml::parse::<FindingSeverity>(text).is_ok() => {}
        Some(text) => into.push(Diagnostic::new(
            subject.clone(),
            Problem::unknown_value(text, LADDER),
        )),
        None => into.push(Diagnostic::new(
            subject.clone(),
            Problem::wrong_shape(shape_of(value), "text", "severity: blocking"),
        )),
    }
}

// --- questions -----------------------------------------------------------

const QUESTION_REQUIRED: &[&str] = &["id", "text", "answer_type", "required"];
const QUESTION_OPTIONAL: &[&str] = &["values"];
const QUESTION_HINTS: &[Hint] = &[
    ("question", "text", "the text a person reads is `text:`"),
    ("prompt", "text", "the text a person reads is `text:`"),
    (
        "type",
        "answer_type",
        "the kind of answer is `answer_type:`",
    ),
    (
        "options",
        "values",
        "the answers a `choice` question allows are `values:`",
    ),
    (
        "choices",
        "values",
        "the answers a `choice` question allows are `values:`",
    ),
    (
        "default",
        "",
        "a question has no default: an unanswered one pauses the run",
    ),
];

impl Shaped for QuestionsFile {
    const KIND: DocumentKind = DocumentKind::Questions;
    const EXAMPLE: &'static str = QUESTIONS_EXAMPLE;

    fn diagnose(value: &Value, into: &mut Vec<Diagnostic>) {
        walk_entries(
            value,
            "questions",
            QUESTIONS_EXAMPLE,
            into,
            diagnose_question,
        );
    }
}

/// One question, named by its own id when that parsed.
fn diagnose_question(index: usize, item: &Value, into: &mut Vec<Diagnostic>) {
    {
        let mut subject = Subject::Question { id: None, index };
        let Some(map) = as_mapping(item, &subject, "a mapping", "- id: theme-source", into) else {
            return;
        };
        if let Some(id) = read_id::<QuestionId>(map, &subject, into) {
            subject = Subject::Question {
                id: Some(id),
                index,
            };
        }
        check_keys(
            map,
            &subject,
            QUESTION_REQUIRED,
            QUESTION_OPTIONAL,
            QUESTION_HINTS,
            into,
        );
        check_string(map, "text", &subject, "text: \"Which theme?\"", into);
        check_string_list(
            map,
            "values",
            &subject,
            "a list of allowed answers",
            "values: [\"teal\", \"amber\"]",
            into,
        );
        if let Some(kind) = map.get("answer_type") {
            const KINDS: [&str; 3] = ["text", "choice", "boolean"];
            match kind.as_str() {
                Some(text) if KINDS.contains(&text) => {}
                Some(text) => into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::unknown_value(text, KINDS),
                )),
                None => into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::wrong_shape(shape_of(kind), "text", "answer_type: boolean"),
                )),
            }
        }
        if let Some(required) = map.get("required") {
            if !matches!(required, Value::Bool(_)) {
                into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::wrong_shape(shape_of(required), "true or false", "required: true"),
                ));
            }
        }
    }
}

//! Reading a document an agent wrote, and publishing the shape it
//! should have written.
//!
//! [`read`] parses with serde on the happy path, so the schema is
//! declared exactly once — in the types — and never restated here. Only
//! when serde refuses does the document get walked a second time, by
//! [`Shaped::diagnose`], whose single job is to name every problem at
//! once in the vocabulary of the document. A reader that corrects one
//! problem per round pays a round per problem; a repair cycle that
//! works that way exhausts its budget before the file is readable.
//!
//! [`Shaped::EXAMPLE`] is the same shape as prose a writer can copy. It
//! is what every door publishes: the block a node's session receives,
//! the `document_shape` tool, `yunta schema`, and the shape carried
//! inside a failed read's own report. One text with four consumers,
//! rather than four texts that drift apart at the first schema change.

use serde::de::DeserializeOwned;

use crate::diagnostic::{
    Diagnostic, DocumentKind, DocumentRef, Malformation, Problem, Report, Subject, ValueShape,
};
use crate::events::FindingSeverity;
use crate::yaml::{Mapping, Value};
use crate::{FindingId, FindingsFile, Ledger, QuestionId, QuestionsFile, TaskId};

/// A document whose shape the system publishes and whose failures it
/// explains.
pub trait Shaped: DeserializeOwned {
    const KIND: DocumentKind;

    /// A complete, valid document of this kind, annotated field by
    /// field. Every door hands this to whoever has to write one; the
    /// test that reads it back through [`read`] is what keeps it true.
    const EXAMPLE: &'static str;

    /// Every problem this document has, in document order. Called only
    /// when serde has already refused the document, so it never has to
    /// build a value — only to explain one.
    fn diagnose(value: &Value, into: &mut Vec<Diagnostic>);
}

/// Reads `bytes` into `T`, or reports every problem the document has.
pub fn read<T: Shaped>(bytes: &[u8], document: DocumentRef) -> Result<T, Report> {
    let one = |problem: Problem| {
        Report::new(
            document.clone(),
            vec![Diagnostic::new(Subject::Document, problem)],
        )
    };

    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(one(Problem::not_yaml(None, "the bytes are not UTF-8")));
    };

    let refusal = match crate::yaml::parse::<T>(text) {
        Ok(value) => return Ok(value),
        Err(error) => error.to_string(),
    };

    // Whether the bytes are YAML at all decides which explanation is
    // honest: a document that never parsed has no entries to blame.
    let Ok(value) = crate::yaml::parse::<Value>(text) else {
        return Err(one(Problem::not_yaml(looks_like(text), refusal)));
    };

    let mut diagnostics = Vec::new();
    T::diagnose(&value, &mut diagnostics);
    if diagnostics.is_empty() {
        // The walk found nothing serde objected to. That is a gap in
        // this module, and it is reported as one rather than swallowed:
        // a read that fails always names at least one problem.
        diagnostics.push(Diagnostic::new(
            Subject::Document,
            Problem::unreadable(refusal),
        ));
    }
    Err(Report::new(document, diagnostics))
}

/// A malformation a writer recognizes, so the diagnostic can name the
/// cause instead of the character the scanner tripped on.
fn looks_like(text: &str) -> Option<Malformation> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("```") {
        return Some(Malformation::MarkdownFence);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(Malformation::JsonDocument);
    }
    // A first line with neither a key nor a list item is prose: an
    // agent explaining the file it is about to write.
    let first = trimmed.lines().find(|line| !line.trim().is_empty())?;
    let looks_structural =
        first.contains(':') || first.trim_start().starts_with('-') || first.starts_with("---");
    (!looks_structural).then_some(Malformation::LeadingProse)
}

// --- walking helpers -----------------------------------------------------
//
// Each returns `None` after reporting, so a walk stops descending into
// a value it could not make sense of while still having said why.

fn shape_of(value: &Value) -> ValueShape {
    match value {
        Value::Null => ValueShape::Null,
        Value::Bool(_) => ValueShape::Bool,
        Value::Number(_) => ValueShape::Number,
        Value::String(_) => ValueShape::String,
        Value::Sequence(_) => ValueShape::Sequence,
        Value::Mapping(_) => ValueShape::Mapping,
        Value::Tagged(_) => ValueShape::Tagged,
    }
}

fn as_mapping<'a>(
    value: &'a Value,
    subject: &Subject,
    expected: &str,
    example: &str,
    into: &mut Vec<Diagnostic>,
) -> Option<&'a Mapping> {
    match value {
        Value::Mapping(mapping) => Some(mapping),
        other => {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(other), expected, example),
            ));
            None
        }
    }
}

fn as_sequence<'a>(
    value: &'a Value,
    subject: &Subject,
    expected: &str,
    example: &str,
    into: &mut Vec<Diagnostic>,
) -> Option<&'a Vec<Value>> {
    match value {
        Value::Sequence(items) => Some(items),
        other => {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(other), expected, example),
            ));
            None
        }
    }
}

/// One key a writer plausibly reaches for: the key they wrote, the key
/// this schema spells it with (empty when the concept has no home here),
/// and the sentence that says so.
type Hint = (&'static str, &'static str, &'static str);

/// Reports every key the type does not accept and every required key the
/// document left out.
///
/// A key a hint redirects is reported once, not also as the absence it
/// caused: telling a writer that `question` is unknown AND that `text` is
/// missing describes one mistake as two, and a reader correcting a list
/// of problems has no way to tell that the second disappears with the
/// first.
fn check_keys(
    mapping: &Mapping,
    subject: &Subject,
    required: &[&str],
    optional: &[&str],
    hints: &[Hint],
    into: &mut Vec<Diagnostic>,
) {
    let valid: Vec<&str> = required.iter().chain(optional).copied().collect();
    let mut already_explained: Vec<&str> = Vec::new();
    for (key, _) in mapping {
        let Some(name) = key.as_str() else {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(key), "a key", valid.first().copied().unwrap_or("")),
            ));
            continue;
        };
        if valid.contains(&name) {
            continue;
        }
        let problem = match hints.iter().find(|(bad, ..)| *bad == name) {
            Some((_, replacement, hint)) => {
                if !replacement.is_empty() {
                    already_explained.push(replacement);
                }
                Problem::unknown_key_instead(name, valid.iter().copied(), *hint)
            }
            None => Problem::unknown_key(name, valid.iter().copied()),
        };
        into.push(Diagnostic::new(subject.clone(), problem));
    }
    for key in required {
        if mapping.get(*key).is_none() && !already_explained.contains(key) {
            into.push(Diagnostic::new(subject.clone(), Problem::missing_key(*key)));
        }
    }
}

/// Reports a value that should have been text and was not.
fn check_string(
    mapping: &Mapping,
    key: &str,
    subject: &Subject,
    example: &str,
    into: &mut Vec<Diagnostic>,
) {
    if let Some(value) = mapping.get(key) {
        if !matches!(value, Value::String(_)) {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(value), "text", example),
            ));
        }
    }
}

/// Reports a list of globs that is not a list of text.
fn check_string_list(
    mapping: &Mapping,
    key: &str,
    subject: &Subject,
    expected: &str,
    example: &str,
    into: &mut Vec<Diagnostic>,
) {
    let Some(value) = mapping.get(key) else {
        return;
    };
    let Some(items) = as_sequence(value, subject, expected, example, into) else {
        return;
    };
    for item in items {
        if !matches!(item, Value::String(_)) {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(item), "text", example),
            ));
        }
    }
}

/// The identifier a document entry declares, when it is readable, plus
/// the diagnostic when it is not. An unreadable id is exactly the case
/// that makes a deserializer's path useless, so the entry falls back to
/// naming itself by position.
fn read_id<T: std::str::FromStr<Err = crate::InvalidId>>(
    mapping: &Mapping,
    subject: &Subject,
    into: &mut Vec<Diagnostic>,
) -> Option<T> {
    let raw = mapping.get("id")?;
    match raw.as_str() {
        Some(text) => match text.parse::<T>() {
            Ok(id) => Some(id),
            Err(invalid) => {
                into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::invalid_id(invalid.value, invalid.rule),
                ));
                None
            }
        },
        None => {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(raw), "text", "id: add-dark-mode"),
            ));
            None
        }
    }
}

/// The one-entry document every interpreted artifact is: a single
/// top-level key holding a list.
fn walk_entries(
    value: &Value,
    key: &'static str,
    example: &'static str,
    into: &mut Vec<Diagnostic>,
    mut entry: impl FnMut(usize, &Value, &mut Vec<Diagnostic>),
) {
    let document = Subject::Document;
    let Some(root) = as_mapping(
        value,
        &document,
        &format!("a mapping with `{key}:`"),
        example,
        into,
    ) else {
        return;
    };
    check_keys(root, &document, &[key], &[], &[], into);
    let Some(list) = root.get(key) else {
        return;
    };
    let Some(items) = as_sequence(
        list,
        &document,
        &format!("a list under `{key}:`"),
        example,
        into,
    ) else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        entry(index, item, into);
    }
}

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
        walk_entries(value, "tasks", LEDGER_EXAMPLE, into, |index, item, into| {
            let mut subject = Subject::Task { id: None, index };
            let Some(map) = as_mapping(item, &subject, "a mapping", "- id: add-dark-mode", into)
            else {
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
        });
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

const LEDGER_EXAMPLE: &str = r#"# A task ledger: one task per independently-verifiable unit of work.
# Written strictly — any key not listed here fails the node.

tasks:                                  # required, at least one
  - id: add-dark-mode                   # required, unique. A letter, then
                                        #   letters, digits, `_` or `-`.
    title: "Add the dark mode toggle"   # required, non-empty
    scope: ["src/theme/**"]             # required, at least one glob: the
                                        #   only paths this task may touch
    criteria:                           # required, at least one
      - cmd: "cargo test -p ui dark_mode"   # required, non-empty. Must FAIL
                                            #   before the work and pass after.
      - cmd: "cargo clippy -- -D warnings"
        type: guard                     # optional. A guard passes before AND
                                        #   after. At least one criterion of a
                                        #   task must not be a guard.

  - id: document-dark-mode
    title: "Document the toggle in the README"
    scope: ["README.md"]                # two tasks with no dependency between
                                        #   them must not overlap in scope
    depends_on: [add-dark-mode]         # optional, ids declared in this file
    notes: "Link the screenshot the first task adds."   # optional, one line of
                                        #   context for a runner with no history
    criteria:
      - cmd: "grep -q 'dark mode' README.md"

# `manual_review: true` marks a task no command can verify, and then also
# requires `justification:`. A criterion is always better: the engine can
# run one, and nobody has to take anyone's word for it.
"#;

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
        walk_entries(
            value,
            "findings",
            FINDINGS_EXAMPLE,
            into,
            |index, item, into| {
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
            },
        );
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

const FINDINGS_EXAMPLE: &str = r#"# What a review found, as data the engine counts and carries forward.
# Written strictly — any key not listed here fails the node.

findings:                               # required; an empty list is valid and
                                        #   means the review found nothing
  - id: null-deref-on-resize            # required, unique in this file
    severity: blocking                  # required, one of:
                                        #   blocking | major | minor | note
    title: "Resize handler dereferences a null pointer"   # required, one line
    location: "src/ui/resize.rs:142-150"                  # required: a path,
                                        #   optionally with a line range
    detail: >-                          # required: what goes wrong, and when
      When the window is resized before the first paint, `surface` is still
      null and the handler dereferences it.
    proposed_criterion:                 # optional: a command that is red today
      cmd: "cargo test -p ui resize_before_paint"
"#;

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
            |index, item, into| {
                let mut subject = Subject::Question { id: None, index };
                let Some(map) = as_mapping(item, &subject, "a mapping", "- id: theme-source", into)
                else {
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
                            Problem::wrong_shape(
                                shape_of(required),
                                "true or false",
                                "required: true",
                            ),
                        ));
                    }
                }
            },
        );
    }
}

const QUESTIONS_EXAMPLE: &str = r#"# What a node needs a person to decide. A node that needs an answer does
# not converse: it writes this file and ends, and the engine asks.
# Written strictly — any key not listed here fails the node.

questions:                              # required; an empty list is valid and
                                        #   means nothing needs deciding
  - id: theme-source                    # required, unique. A letter, then
                                        #   letters, digits, `_` or `-`.
    text: "Should dark mode follow the operating system setting?"  # required
    answer_type: boolean                # required, one of:
                                        #   text | choice | boolean
    required: true                      # required: an unanswered `required`
                                        #   question pauses the run

  - id: accent-colour
    text: "Which accent colour should the dark theme use?"
    answer_type: choice
    values: ["teal", "amber", "violet"] # required when answer_type is `choice`,
                                        #   and an answer must be one of these
    required: false
"#;

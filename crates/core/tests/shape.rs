//! Reading a document an agent wrote: every problem reported at once,
//! named the way the document names things, and the shape the document
//! should have had available to whoever has to write it.

use yunta_core::shape::{read, Shaped};
use yunta_core::{DocumentKind, DocumentRef, FindingsFile, Ledger, QuestionsFile};

fn plan() -> DocumentRef {
    DocumentRef::new(DocumentKind::TaskLedger, "artifacts/plan.yaml")
}

fn findings() -> DocumentRef {
    DocumentRef::new(DocumentKind::Findings, "artifacts/findings.yaml")
}

fn questions() -> DocumentRef {
    DocumentRef::new(DocumentKind::Questions, "artifacts/questions.yaml")
}

// --- the example is the shape, and it stays true -----------------------
//
// Every door publishes `EXAMPLE`. These are what stop it drifting from
// the parser: a change to a schema that leaves the example behind fails
// here, before the merge.

#[test]
fn the_published_ledger_shape_is_a_ledger_this_parser_accepts() {
    read::<Ledger>(Ledger::EXAMPLE.as_bytes(), plan()).expect("the published shape parses");
}

#[test]
fn the_published_findings_shape_is_a_findings_file_this_parser_accepts() {
    read::<FindingsFile>(FindingsFile::EXAMPLE.as_bytes(), findings())
        .expect("the published shape parses");
}

#[test]
fn the_published_questions_shape_is_a_questions_file_this_parser_accepts() {
    read::<QuestionsFile>(QuestionsFile::EXAMPLE.as_bytes(), questions())
        .expect("the published shape parses");
}

#[test]
fn every_published_shape_carries_its_own_top_level_key() {
    assert!(Ledger::EXAMPLE.contains("tasks:"));
    assert!(FindingsFile::EXAMPLE.contains("findings:"));
    assert!(QuestionsFile::EXAMPLE.contains("questions:"));
}

// --- a good document still reads -----------------------------------------

#[test]
fn a_well_formed_ledger_reads_into_its_type() {
    let ledger = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        plan(),
    )
    .expect("a well-formed ledger reads");
    assert_eq!(ledger.tasks.len(), 1);
    assert_eq!(ledger.tasks[0].id.to_string(), "t1");
}

// --- every problem at once ------------------------------------------------

#[test]
fn a_ledger_with_two_problems_reports_both_in_one_read() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    description: extra\n    scope: [\"src/**\"]\n    criteria:\n      - cargo test\n",
        plan(),
    )
    .expect_err("two problems");
    let text = report.for_person();
    assert_eq!(
        report.diagnostics.len(),
        2,
        "one round must surface both: {text}"
    );
    assert!(text.contains("unknown key `description`"), "{text}");
    assert!(text.contains("criterion 1"), "{text}");
}

// --- the subject is never a deserializer path -----------------------------

#[test]
fn no_rendering_ever_shows_the_path_a_deserializer_walked() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cargo test\n",
        plan(),
    )
    .expect_err("a criterion written as plain text");
    let text = report.for_person();
    assert!(!text.contains("tasks["), "{text}");
    assert!(!text.contains("Criterion"), "no Rust type names: {text}");
    assert!(text.contains("task `t1`, criterion 1"), "{text}");
}

#[test]
fn a_task_whose_id_is_unreadable_is_still_named_by_its_position() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: 1-dark-mode\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        plan(),
    )
    .expect_err("an id that breaks the rule");
    let text = report.for_person();
    assert!(text.contains("the first task"), "{text}");
    assert!(!text.contains("tasks["), "{text}");
}

// --- shapes a writer actually gets wrong ----------------------------------

#[test]
fn a_criterion_written_as_plain_text_says_what_to_write_instead() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cargo test\n",
        plan(),
    )
    .expect_err("a criterion is a mapping");
    let text = report.for_person();
    assert!(text.contains("cmd:"), "{text}");
}

#[test]
fn a_top_level_key_that_does_not_exist_names_the_one_that_does() {
    let report = read::<Ledger>(b"version: 1\ntasks: []\n", plan()).expect_err("unknown key");
    let text = report.for_person();
    assert!(text.contains("unknown key `version`"), "{text}");
    assert!(text.contains("`tasks`"), "{text}");
}

#[test]
fn a_missing_required_key_is_named_as_missing() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"x\"\n",
        plan(),
    )
    .expect_err("no title");
    let text = report.for_person();
    assert!(text.contains("`title`"), "{text}");
    assert!(text.contains("task `t1`"), "{text}");
}

#[test]
fn a_markdown_fence_is_recognized_instead_of_blamed_on_a_character() {
    let report = read::<Ledger>(b"```yaml\ntasks: []\n```\n", plan()).expect_err("a fence");
    let text = report.for_person();
    assert!(text.contains("Markdown code fence"), "{text}");
    assert!(!text.contains("cannot start any token"), "{text}");
}

#[test]
fn a_json_document_is_recognized_as_json() {
    let report = read::<Ledger>(b"{\n\"tasks\": [,]\n}\n", plan()).expect_err("json");
    let text = report.for_person();
    assert!(text.contains("JSON"), "{text}");
}

// --- findings and questions get the same treatment ------------------------

#[test]
fn a_findings_severity_outside_the_ladder_lists_the_ladder() {
    let report = read::<FindingsFile>(
        b"findings:\n  - id: F1\n    severity: high\n    title: T\n    location: a.rs\n    detail: D\n",
        findings(),
    )
    .expect_err("severity high does not exist");
    let text = report.for_person();
    assert!(text.contains("blocking"), "{text}");
    assert!(text.contains("finding `F1`"), "{text}");
}

#[test]
fn a_question_with_the_wrong_key_names_the_key_that_exists() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    question: Why?\n    answer_type: text\n    required: true\n",
        questions(),
    )
    .expect_err("`question` is not a key");
    let text = report.for_person();
    assert!(text.contains("unknown key `question`"), "{text}");
    assert!(text.contains("`text`"), "{text}");
}

// --- nothing is ever swallowed -------------------------------------------

#[test]
fn a_document_the_walk_cannot_explain_still_fails_with_a_diagnostic() {
    // A shape no rule above covers: `tasks` is a mapping, not a list.
    let report = read::<Ledger>(b"tasks:\n  t1:\n    title: Work\n", plan())
        .expect_err("tasks is not a list");
    assert!(
        !report.diagnostics.is_empty(),
        "a failed read always names at least one problem"
    );
}

// --- the wording itself ---------------------------------------------------

#[test]
fn a_problem_with_the_file_itself_reads_as_one_sentence() {
    let report = read::<Ledger>(b"version: 1\ntasks: []\n", plan()).expect_err("unknown key");
    assert_eq!(
        report.for_person(),
        "artifacts/plan.yaml: 1 error\n  \
         the document: unknown key `version`; the only top-level key is `tasks`"
    );
}

#[test]
fn a_misspelled_key_is_reported_once_not_also_as_the_absence_it_caused() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    question: Why?\n    answer_type: text\n    required: true\n",
        questions(),
    )
    .expect_err("`question` is not a key");
    let text = report.for_person();
    assert_eq!(report.diagnostics.len(), 1, "one mistake, one line: {text}");
    assert!(
        text.contains("the text a person reads is `text:`"),
        "{text}"
    );
}

#[test]
fn a_criterion_still_says_which_task_when_that_task_s_id_is_unreadable() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: 1-dark-mode\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cargo test\n",
        plan(),
    )
    .expect_err("an unreadable id and a bad criterion");
    let text = report.for_person();
    assert!(
        text.contains("the first task, criterion 1"),
        "a reader with no id still has somewhere to look: {text}"
    );
}

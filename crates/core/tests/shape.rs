//! Reading a document an agent wrote: every problem reported at once,
//! named the way the document names things, and the shape the document
//! should have had available to whoever has to write it.

use yunta_core::shape::{read, Document};
use yunta_core::{FindingsFile, Ledger, QuestionsFile};

const PLAN: &str = "artifacts/plan.yaml";
const FINDINGS: &str = "artifacts/findings.yaml";
const QUESTIONS: &str = "artifacts/questions.yaml";

// --- the example is the shape, and it stays true -----------------------
//
// Every door publishes `EXAMPLE`. These are what stop it drifting from
// the parser: a change to a schema that leaves the example behind fails
// here, before the merge.

#[test]
fn the_published_ledger_shape_is_a_ledger_this_parser_accepts() {
    read::<Ledger>(<Ledger as Document>::EXAMPLE.as_bytes(), PLAN)
        .expect("the published shape parses");
}

#[test]
fn the_published_findings_shape_is_a_findings_file_this_parser_accepts() {
    read::<FindingsFile>(<FindingsFile as Document>::EXAMPLE.as_bytes(), FINDINGS)
        .expect("the published shape parses");
}

#[test]
fn the_published_questions_shape_is_a_questions_file_this_parser_accepts() {
    read::<QuestionsFile>(<QuestionsFile as Document>::EXAMPLE.as_bytes(), QUESTIONS)
        .expect("the published shape parses");
}

#[test]
fn every_published_shape_carries_its_own_top_level_key() {
    assert!(<Ledger as Document>::EXAMPLE.contains("tasks:"));
    assert!(<FindingsFile as Document>::EXAMPLE.contains("findings:"));
    assert!(<QuestionsFile as Document>::EXAMPLE.contains("questions:"));
}

// --- a good document still reads -----------------------------------------

#[test]
fn a_well_formed_ledger_reads_into_its_type() {
    let ledger = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
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
        PLAN,
    )
    .expect_err("two problems");
    let text = report.to_string();
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
        PLAN,
    )
    .expect_err("a criterion written as plain text");
    let text = report.to_string();
    assert!(!text.contains("tasks["), "{text}");
    assert!(!text.contains("Criterion"), "no Rust type names: {text}");
    assert!(text.contains("task `t1`, criterion 1"), "{text}");
}

#[test]
fn a_task_whose_id_is_unreadable_is_still_named_by_its_position() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: 1-dark-mode\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("an id that breaks the rule");
    let text = report.to_string();
    assert!(text.contains("the first task"), "{text}");
    assert!(!text.contains("tasks["), "{text}");
}

// --- shapes a writer actually gets wrong ----------------------------------

#[test]
fn a_criterion_written_as_plain_text_says_what_to_write_instead() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cargo test\n",
        PLAN,
    )
    .expect_err("a criterion is a mapping");
    let text = report.to_string();
    assert!(text.contains("cmd:"), "{text}");
}

#[test]
fn a_top_level_key_that_does_not_exist_names_the_one_that_does() {
    let report = read::<Ledger>(b"version: 1\ntasks: []\n", PLAN).expect_err("unknown key");
    let text = report.to_string();
    assert!(text.contains("unknown key `version`"), "{text}");
    assert!(text.contains("`tasks`"), "{text}");
}

#[test]
fn a_missing_required_key_is_named_as_missing() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"x\"\n",
        PLAN,
    )
    .expect_err("no title");
    let text = report.to_string();
    assert!(text.contains("`title`"), "{text}");
    assert!(text.contains("task `t1`"), "{text}");
}

#[test]
fn a_markdown_fence_is_recognized_instead_of_blamed_on_a_character() {
    let report = read::<Ledger>(b"```yaml\ntasks: []\n```\n", PLAN).expect_err("a fence");
    let text = report.to_string();
    assert!(text.contains("Markdown code fence"), "{text}");
    assert!(!text.contains("cannot start any token"), "{text}");
}

#[test]
fn a_json_document_is_recognized_as_json() {
    let report = read::<Ledger>(b"{\n\"tasks\": [,]\n}\n", PLAN).expect_err("json");
    let text = report.to_string();
    assert!(text.contains("JSON"), "{text}");
}

// --- findings and questions get the same treatment ------------------------

#[test]
fn a_findings_severity_outside_the_ladder_lists_the_ladder() {
    let report = read::<FindingsFile>(
        b"findings:\n  - id: F1\n    severity: high\n    title: T\n    location: a.rs\n    detail: D\n",
        FINDINGS,
    )
    .expect_err("severity high does not exist");
    let text = report.to_string();
    assert!(text.contains("blocking"), "{text}");
    assert!(text.contains("finding `F1`"), "{text}");
}

#[test]
fn a_question_with_the_wrong_key_names_the_key_that_exists() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    question: Why?\n    answer_type: text\n    required: true\n",
        QUESTIONS,
    )
    .expect_err("`question` is not a key");
    let text = report.to_string();
    assert!(text.contains("unknown key `question`"), "{text}");
    assert!(text.contains("`text`"), "{text}");
}

// --- nothing is ever swallowed -------------------------------------------

#[test]
fn a_document_the_walk_cannot_explain_still_fails_with_a_diagnostic() {
    // A shape no rule above covers: `tasks` is a mapping, not a list.
    let report =
        read::<Ledger>(b"tasks:\n  t1:\n    title: Work\n", PLAN).expect_err("tasks is not a list");
    assert!(
        !report.diagnostics.is_empty(),
        "a failed read always names at least one problem"
    );
}

// --- the wording itself ---------------------------------------------------

#[test]
fn a_problem_with_the_file_itself_reads_as_one_sentence() {
    let report = read::<Ledger>(b"version: 1\ntasks: []\n", PLAN).expect_err("unknown key");
    assert_eq!(
        report.to_string(),
        "artifacts/plan.yaml: 1 error\n  \
         the document: unknown key `version`; the only top-level key is `tasks`"
    );
}

#[test]
fn a_misspelled_key_is_reported_once_not_also_as_the_absence_it_caused() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    question: Why?\n    answer_type: text\n    required: true\n",
        QUESTIONS,
    )
    .expect_err("`question` is not a key");
    let text = report.to_string();
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
        PLAN,
    )
    .expect_err("an unreadable id and a bad criterion");
    let text = report.to_string();
    assert!(
        text.contains("the first task, criterion 1"),
        "a reader with no id still has somewhere to look: {text}"
    );
}

// --- one door, shape and rules together -----------------------------------
//
// A document that parses and breaks a rule fails the same read as one
// that never parsed. There is no second way to obtain a ledger, so no
// caller can hold one whose rules were never asked.

#[test]
fn a_ledger_that_parses_and_breaks_a_rule_fails_the_same_read() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria: []\n",
        PLAN,
    )
    .expect_err("a task with no criteria");
    let text = report.to_string();
    assert!(text.contains("task `t1`"), "{text}");
    assert!(text.contains("no criteria"), "{text}");
    assert_eq!(report.diagnostics[0].code(), "no-criteria");
}

#[test]
fn a_ledger_naming_a_task_nobody_declared_fails_the_read() {
    let report = read::<Ledger>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    depends_on: [ghost]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("a dependency on nothing");
    assert_eq!(report.diagnostics[0].code(), "unknown-dependency");
}

#[test]
fn a_findings_file_with_one_id_twice_fails_the_read() {
    let entry =
        "  - id: F1\n    severity: blocking\n    title: T\n    location: a.rs\n    detail: D\n";
    let bytes = format!("findings:\n{entry}{entry}");
    let report = read::<FindingsFile>(bytes.as_bytes(), FINDINGS).expect_err("one id twice");
    assert_eq!(report.diagnostics[0].code(), "duplicate-id");
    assert_eq!(
        report.document.kind,
        yunta_core::ArtifactKind::Findings,
        "the report says which kind of document it is about"
    );
}

#[test]
fn a_choice_question_with_nothing_to_choose_fails_the_read() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    text: Which?\n    answer_type: choice\n    required: true\n",
        QUESTIONS,
    )
    .expect_err("a choice with no values");
    assert_eq!(report.diagnostics[0].code(), "missing-values");
}

#[test]
fn a_read_names_the_document_by_the_type_that_read_it() {
    // The kind comes from the type, so a report about a ledger can
    // never publish the shape of a questions file.
    let report = read::<Ledger>(b"version: 1\ntasks: []\n", PLAN).expect_err("unknown key");
    assert_eq!(report.document.kind, yunta_core::ArtifactKind::TaskLedger);
    assert_eq!(report.document.path, PLAN);
}

#[test]
fn a_finding_whose_text_fields_are_blank_names_each_one() {
    let report = read::<FindingsFile>(
        b"findings:\n  - id: F1\n    severity: blocking\n    title: \"  \"\n    location: \"\"\n    detail: \"\"\n",
        FINDINGS,
    )
    .expect_err("three empty fields a String cannot refuse on its own");
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code()).collect();
    assert_eq!(codes, ["empty-title", "empty-location", "empty-detail"]);
    assert!(report.to_string().contains("finding `F1`"));
}

#[test]
fn a_question_nobody_can_read_fails_the_read() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    text: \"   \"\n    answer_type: text\n    required: true\n",
        QUESTIONS,
    )
    .expect_err("a blank question");
    assert_eq!(report.diagnostics[0].code(), "empty-text");
}

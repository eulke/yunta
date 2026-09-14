//! Reading a document an agent wrote: the rules it broke reported
//! together, a value it got wrong located by its path, and the shape it
//! should have had available to whoever writes one.

use yunta_core::shape::{read, Document};
use yunta_core::{
    Answer, AnswersFile, FindingEntry, FindingsFile, QuestionsFile, TasksFile, Withdrawal,
};

const PLAN: &str = "artifacts/plan.yaml";
const FINDINGS: &str = "artifacts/findings.yaml";
const QUESTIONS: &str = "artifacts/questions.yaml";
const ANSWERS: &str = "artifacts/answers.yaml";
const POST: &str = "yunta_post_finding";
const WITHDRAW: &str = "yunta_withdraw_finding";

// --- the example is the shape, and it stays true -----------------------
//
// Every door publishes `EXAMPLE`. These are what stop it drifting from
// the parser: a change to a schema that leaves the example behind fails
// here, before the merge.

#[test]
fn the_published_tasks_shape_is_a_document_this_parser_accepts() {
    read::<TasksFile>(<TasksFile as Document>::EXAMPLE.as_bytes(), PLAN)
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
    assert!(<TasksFile as Document>::EXAMPLE.contains("tasks:"));
    assert!(<FindingsFile as Document>::EXAMPLE.contains("findings:"));
    assert!(<QuestionsFile as Document>::EXAMPLE.contains("questions:"));
}

// --- a good document still reads -----------------------------------------

#[test]
fn a_well_formed_tasks_document_reads_into_its_type() {
    let tasks = read::<TasksFile>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect("a well-formed tasks document reads");
    assert_eq!(tasks.tasks.len(), 1);
    assert_eq!(tasks.tasks[0].id.to_string(), "t1");
}

// --- every rule at once ---------------------------------------------------
//
// A document that parses is held to every rule of its kind in one read:
// a writer who hears one rule per round pays a round per rule.

#[test]
fn a_tasks_document_that_breaks_two_rules_reports_both_in_one_read() {
    let report = read::<TasksFile>(
        b"tasks:\n  - id: a\n    title: Work\n    scope: [\"src/**\"]\n    depends_on: [ghost]\n    criteria:\n      - cmd: \"cargo test\"\n  - id: b\n    title: More\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("two rules");
    let codes: Vec<&str> = report
        .diagnostics
        .iter()
        .map(|d| d.problem.code().as_str())
        .collect();
    assert!(
        codes.contains(&"unknown-dependency") && codes.contains(&"overlapping-scope"),
        "one read surfaces both: {codes:?}"
    );
}

#[test]
fn a_rule_names_its_subject_the_way_the_document_names_it() {
    let report = read::<TasksFile>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n        type: guard\n",
        PLAN,
    )
    .expect_err("every criterion is a guard");
    let text = report.to_string();
    assert!(text.contains("task `t1`"), "named by its id: {text}");
    assert!(!text.contains("Task {"), "no Rust type names: {text}");
}

// --- a value the document got wrong is located ----------------------------
//
// What a writer needs is which value, and what was expected of it. The
// path from the document's root says the first; the deserializer's own
// account of the value says the second.

#[test]
fn a_value_of_the_wrong_type_is_located_by_its_path() {
    let report = read::<TasksFile>(
        b"tasks:\n  - id: t1\n    title: Work\n    scope: [\"src/**\"]\n    manual_review: yes please\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("a string where a boolean belongs");
    let text = report.to_string();
    assert!(text.contains("tasks[0].manual_review"), "{text}");
}

#[test]
fn a_key_the_type_does_not_declare_is_named() {
    let report = read::<TasksFile>(
        b"tasks:\n  - id: t1\n    title: Work\n    description: extra\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("a key nobody declared");
    let text = report.to_string();
    assert!(text.contains("description"), "the key itself: {text}");
}

#[test]
fn answers_read_through_the_same_door_as_every_document() {
    // The engine writes the answers, and reads them back the way it
    // reads what an agent wrote: one door, one report. A document whose
    // shape is wrong says so at the value, and a rule it breaks says so
    // at the entry that broke it.
    read::<AnswersFile>(<AnswersFile as Document>::EXAMPLE.as_bytes(), ANSWERS)
        .expect("the published shape parses");

    let report = read::<AnswersFile>(
        b"answers:\n  - id: theme\n    value: dark\n  - id: theme\n    value: light\n",
        ANSWERS,
    )
    .expect_err("one id answers one question");
    let text = report.to_string();
    assert_eq!(
        report.diagnostics[0].problem.code().as_str(),
        "duplicate-id"
    );
    assert!(text.contains("theme"), "{text}");

    let report = read::<AnswersFile>(b"answers:\n  - id: theme\n", ANSWERS)
        .expect_err("an answer carries a value");
    assert_eq!(report.diagnostics[0].problem.code().as_str(), "parse");
    assert!(report.to_string().contains("answers[0]"), "{report}");
}

#[test]
fn a_reply_is_judged_against_the_questions_it_answers() {
    // `read` sees one document; the questions are another, so what a
    // reply owes them is asked where both are in hand. Every violation
    // together, never the first.
    let questions: QuestionsFile = read(
        br#"
questions:
  - id: theme
    text: Which theme?
    answer_type: choice
    values: [dark, light]
    required: true
  - id: ship
    text: Ship it?
    answer_type: boolean
    required: true
"#,
        QUESTIONS,
    )
    .expect("the questions read");

    let report = AnswersFile::against(
        &questions,
        vec![
            Answer {
                id: "theme".into(),
                value: "purple".to_string(),
            },
            Answer {
                id: "ghost".into(),
                value: "x".to_string(),
            },
        ],
    )
    .expect_err("three ways at once");
    let text = report.to_string();
    assert!(text.contains("purple"), "the value off the list: {text}");
    assert!(text.contains("ghost"), "the question nobody asked: {text}");
    assert!(
        text.contains("ship"),
        "the required question unanswered: {text}"
    );

    AnswersFile::against(
        &questions,
        vec![
            Answer {
                id: "theme".into(),
                value: "dark".to_string(),
            },
            Answer {
                id: "ship".into(),
                value: "true".to_string(),
            },
        ],
    )
    .expect("a reply that answers its questions");
}

#[test]
fn a_value_outside_a_closed_set_lists_the_set() {
    let report = read::<FindingsFile>(
        b"findings:\n  - id: f1\n    severity: high\n    title: T\n    location: a.rs:1\n    detail: D\n",
        FINDINGS,
    )
    .expect_err("a severity off the ladder");
    let text = report.to_string();
    assert!(text.contains("findings[0].severity"), "{text}");
    for rung in ["blocking", "major", "minor", "note"] {
        assert!(text.contains(rung), "the ladder, in full: {text}");
    }
}

#[test]
fn a_finding_entry_reads_through_the_document_door() {
    // A session reports one finding at a time, so a single entry is a
    // document at that frontier — read by the same door, refused with
    // the same report, and publishing the same shape.
    read::<FindingEntry>(<FindingEntry as Document>::EXAMPLE.as_bytes(), POST)
        .expect("the published entry shape parses");

    let report = read::<FindingEntry>(
        b"id: f1\nseverity: major\ntitle: \"\"\nlocation: src/lib.rs\ndetail: D\n",
        POST,
    )
    .expect_err("a title says something");
    assert_eq!(
        report.diagnostics[0].problem.code().as_str(),
        "empty-title",
        "{report}"
    );

    let report = read::<FindingEntry>(b"id: f1\nseverity: major\nnote: x\n", POST)
        .expect_err("a key nobody declared");
    assert_eq!(report.diagnostics[0].problem.code().as_str(), "parse");
    assert!(report.to_string().contains("note"), "{report}");

    // And a withdrawal the same way.
    read::<Withdrawal>(<Withdrawal as Document>::EXAMPLE.as_bytes(), WITHDRAW)
        .expect("the published withdrawal shape parses");
    let report = read::<Withdrawal>(b"id: f1\nreason: \"  \"\n", WITHDRAW)
        .expect_err("a withdrawal says why");
    assert_eq!(
        report.diagnostics[0].problem.code().as_str(),
        "empty-reason",
        "{report}"
    );
}

#[test]
fn a_location_that_does_not_read_is_a_parse_problem_at_its_path() {
    // A location is parsed where it arrives, so what does not read is
    // reported like any other value the door refuses: named at its own
    // key, with the reason the type gives — never a rule of its own
    // over a string that was let in.
    for (location, reason) in [
        (
            "/etc/passwd",
            "an absolute path is this host's, not this run's",
        ),
        ("../../etc/passwd", "climbs out"),
        ("a.rs:14-10", "ends before it starts"),
    ] {
        let yaml = format!(
            "findings:\n  - id: f1\n    severity: major\n    title: T\n    location: \"{location}\"\n    detail: D\n"
        );
        let report = read::<FindingsFile>(yaml.as_bytes(), FINDINGS)
            .expect_err("a location that does not read");
        assert_eq!(report.diagnostics.len(), 1, "{report}");
        assert_eq!(
            report.diagnostics[0].problem.code().as_str(),
            "parse",
            "{report}"
        );
        let text = report.to_string();
        assert!(text.contains("findings[0].location"), "at its key: {text}");
        assert!(text.contains(reason), "with the reason: {text}");
    }
}

#[test]
fn an_id_that_breaks_its_rule_says_what_an_id_is() {
    let report = read::<TasksFile>(
        b"tasks:\n  - id: 1-dark-mode\n    title: Work\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("an id that breaks the rule");
    let text = report.to_string();
    assert!(text.contains("tasks[0].id"), "{text}");
    assert!(text.contains("letter"), "the rule itself: {text}");
}

#[test]
fn a_missing_required_key_is_named() {
    let report = read::<QuestionsFile>(
        b"questions:\n  - id: q1\n    text: Which?\n    required: true\n",
        QUESTIONS,
    )
    .expect_err("no answer_type");
    let text = report.to_string();
    assert!(text.contains("answer_type"), "{text}");
}

// --- nothing is ever swallowed -------------------------------------------

#[test]
fn bytes_that_are_not_yaml_at_all_still_fail_with_a_diagnostic() {
    let report = read::<TasksFile>(b"```yaml\ntasks: []\n```\n", PLAN)
        .expect_err("a fenced document is not YAML");
    assert_eq!(report.diagnostics.len(), 1, "{report}");
    assert_eq!(report.diagnostics[0].problem.code().as_str(), "parse");
}

#[test]
fn bytes_that_are_not_utf8_fail_as_the_document_they_are_not() {
    let report = read::<TasksFile>(&[0xff, 0xfe, 0x00], PLAN).expect_err("not UTF-8");
    assert!(report.to_string().contains("UTF-8"), "{report}");
}

// --- the wording itself ---------------------------------------------------

#[test]
fn a_report_counts_its_problems_in_whole_words() {
    let one = read::<TasksFile>(
        b"tasks:\n  - id: t1\n    title: \"\"\n    scope: [\"src/**\"]\n    criteria:\n      - cmd: \"cargo test\"\n",
        PLAN,
    )
    .expect_err("one problem");
    assert!(one.to_string().contains("1 error"), "{one}");
}

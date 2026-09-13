//! What a document promises in both directions: a value the engine
//! holds renders to YAML that reads back as the same value, and a
//! document a session submits becomes that same value.
//!
//! Written as properties rather than examples because the promise is
//! about every document of a kind, not three of them. The strategies
//! build documents that already satisfy their rules — what is under test
//! is the round trip, and a document the rules refuse never reaches it.

use proptest::prelude::*;
use yunta_core::shape::{accept, read, render};
use yunta_core::{FindingsFile, QuestionsFile, TasksFile};

/// Ids the newtypes accept: a letter, then letters, digits, `_` or `-`.
fn id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_-]{0,12}"
}

fn nonempty() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9 ._/:-]{0,40}"
}

/// Distinct ids, so `DuplicateId` never fires: the round trip is what is
/// under test, not the rules.
fn distinct_ids(max: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::hash_set(id(), 1..=max).prop_map(|set| {
        let mut ids: Vec<String> = set.into_iter().collect();
        ids.sort();
        ids
    })
}

fn tasks() -> impl Strategy<Value = TasksFile> {
    distinct_ids(4).prop_flat_map(|ids| {
        let tasks: Vec<_> = ids
            .into_iter()
            .map(|id| {
                (
                    Just(id),
                    nonempty(),
                    nonempty(),
                    prop::option::of(nonempty()),
                )
            })
            .collect();
        tasks.prop_map(|tasks| {
            let yaml = tasks
                .into_iter()
                .map(|(id, title, cmd, notes)| {
                    let notes = notes
                        .map(|n| format!("    notes: \"{n}\"\n"))
                        .unwrap_or_default();
                    // Each task gets its own scope: two independent
                    // tasks reaching for the same files is a rule
                    // break, and what is under test here is the round
                    // trip, not the rules.
                    format!(
                        "  - id: {id}\n    title: \"{title}\"\n    scope: [\"src/{id}/**\"]\n\
                         {notes}    criteria:\n      - cmd: \"{cmd}\"\n"
                    )
                })
                .collect::<String>();
            read::<TasksFile>(format!("tasks:\n{yaml}").as_bytes(), "generated")
                .expect("the strategy builds a tasks document its own rules accept")
        })
    })
}

fn findings() -> impl Strategy<Value = FindingsFile> {
    distinct_ids(4).prop_flat_map(|ids| {
        let entries: Vec<_> = ids
            .into_iter()
            .map(|id| (Just(id), nonempty(), nonempty(), nonempty()))
            .collect();
        entries.prop_map(|entries| {
            let yaml = entries
                .into_iter()
                .map(|(id, title, location, detail)| {
                    format!(
                        "  - id: {id}\n    severity: major\n    title: \"{title}\"\n\
                         \x20   location: \"{location}\"\n    detail: \"{detail}\"\n"
                    )
                })
                .collect::<String>();
            read::<FindingsFile>(format!("findings:\n{yaml}").as_bytes(), "generated")
                .expect("the strategy builds findings its own rules accept")
        })
    })
}

fn questions() -> impl Strategy<Value = QuestionsFile> {
    distinct_ids(4).prop_flat_map(|ids| {
        let entries: Vec<_> = ids
            .into_iter()
            .map(|id| (Just(id), nonempty(), any::<bool>()))
            .collect();
        entries.prop_map(|entries| {
            let yaml = entries
                .into_iter()
                .map(|(id, text, required)| {
                    format!(
                        "  - id: {id}\n    text: \"{text}\"\n    answer_type: text\n\
                         \x20   required: {required}\n"
                    )
                })
                .collect::<String>();
            read::<QuestionsFile>(format!("questions:\n{yaml}").as_bytes(), "generated")
                .expect("the strategy builds questions its own rules accept")
        })
    })
}

/// The three promises, for one document: the rendered text reads back as
/// the same value; the value submitted as JSON accepts as the same
/// value; and rendering is a function of the value alone, so the same
/// meaning is the same bytes and the same hash.
macro_rules! round_trips {
    ($name:ident, $type:ty, $strategy:expr) => {
        proptest! {
            #[test]
            fn $name(document in $strategy) {
                let text = render(&document).expect("a held document renders");
                let read_back = read::<$type>(text.as_bytes(), "rendered")
                    .expect("what render wrote, read reads");
                prop_assert_eq!(&read_back, &document);

                let json = serde_json::to_value(&document).expect("a held document is JSON");
                let accepted = accept::<$type>(json, "submitted")
                    .expect("what the engine holds, accept accepts");
                prop_assert_eq!(&accepted, &document);

                prop_assert_eq!(
                    render(&accepted).expect("an accepted document renders"),
                    text
                );
            }
        }
    };
}

round_trips!(a_tasks_document_round_trips, TasksFile, tasks());
round_trips!(findings_round_trip, FindingsFile, findings());
round_trips!(questions_round_trip, QuestionsFile, questions());

#[test]
fn accept_names_the_path_of_the_value_it_refused() {
    let document = serde_json::json!({
        "tasks": [{
            "id": "t1",
            "title": "t",
            "scope": ["src/**"],
            "criteria": [{"cmd": "true"}],
            "manual_review": "yes",
        }]
    });
    let report = accept::<TasksFile>(document, "artifacts/plan.yaml")
        .expect_err("a string where a boolean belongs is refused");
    let rendered = report.diagnostics[0].to_string();
    assert!(
        rendered.contains("tasks[0].manual_review"),
        "the path locates the value: {rendered}"
    );
}

#[test]
fn accept_refuses_a_key_the_type_does_not_declare() {
    let document = serde_json::json!({
        "findings": [{
            "id": "f1",
            "severity": "major",
            "title": "t",
            "location": "a.rs:1",
            "detail": "d",
            "line": 42,
        }]
    });
    let report = accept::<FindingsFile>(document, "yunta_post_finding")
        .expect_err("an unknown key is refused");
    let rendered = report.diagnostics[0].to_string();
    assert!(
        rendered.contains("line"),
        "the refusal names the key: {rendered}"
    );
}

#[test]
fn accept_reports_the_rules_of_a_document_that_parsed() {
    let document = serde_json::json!({
        "tasks": [
            {
                "id": "a",
                "title": "a",
                "scope": ["src/**"],
                "criteria": [{"cmd": "true"}],
                "depends_on": ["nobody"],
            },
        ]
    });
    let report = accept::<TasksFile>(document, "artifacts/plan.yaml")
        .expect_err("a dependency on a task nobody declared is refused");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.problem.code() == "unknown-dependency"),
        "the report names the rule: {report:?}"
    );
}

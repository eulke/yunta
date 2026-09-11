//! `yunta schema`: the shell door onto the shape of every document
//! Yunta reads.
//!
//! The point of this door is that it needs nothing: no project, no run,
//! no control plane. An agent working in a repo without MCP, and a
//! person writing a ledger by hand, both had no way to learn the format
//! before it — the files under `schemas/` are produced by this
//! repository's own development tooling and never reach an installed
//! binary.

use yunta_testkit::{stderr, stdout, yunta_in};

#[test]
fn with_no_arguments_it_lists_every_document_yunta_reads() {
    let here = tempfile::tempdir().unwrap();
    let output = yunta_in!(here.path(), here.path(), &["schema"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    for kind in ["task-ledger", "findings", "questions"] {
        assert!(text.contains(kind), "{kind} missing from {text}");
    }
}

#[test]
fn a_kind_prints_the_shape_a_writer_copies() {
    let here = tempfile::tempdir().unwrap();
    let output = yunta_in!(here.path(), here.path(), &["schema", "task-ledger"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    // Every required key, and the rule a writer cannot infer.
    for needle in ["tasks:", "id:", "title:", "scope:", "criteria:", "cmd:"] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    assert!(
        text.contains("must not be a guard"),
        "the rules travel with the shape: {text}"
    );
}

#[test]
fn the_shape_needs_no_project_around_it() {
    // An empty directory: no `.yunta/`, no config, no run. Learning the
    // format cannot depend on having set anything up.
    let here = tempfile::tempdir().unwrap();
    for kind in ["task-ledger", "findings", "questions"] {
        let output = yunta_in!(here.path(), here.path(), &["schema", kind]);
        assert!(output.status.success(), "{kind}: {}", stderr(&output));
        assert!(!stdout(&output).trim().is_empty(), "{kind} printed nothing");
    }
}

#[test]
fn json_emits_the_schema_an_editor_validates_against() {
    let here = tempfile::tempdir().unwrap();
    let output = yunta_in!(here.path(), here.path(), &["schema", "findings", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("--json emits JSON");
    assert_eq!(parsed["title"], "yunta findings");
    assert!(parsed["properties"]["findings"].is_object());
}

#[test]
fn a_kind_that_does_not_exist_names_the_ones_that_do() {
    let here = tempfile::tempdir().unwrap();
    let output = yunta_in!(here.path(), here.path(), &["schema", "ledger"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("task-ledger"), "{text}");
}

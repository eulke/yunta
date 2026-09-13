//! `yunta schema`: the shell door onto the shape of every document
//! Yunta reads.
//!
//! The point of this door is that it needs nothing: no project, no run,
//! no control plane. It is how an agent working in a repo without MCP,
//! and a person writing a tasks document by hand, learn the format at all —
//! [`COMMITTED_SCHEMAS`] is a development artifact of this repository,
//! and whoever installed the binary has no repository to read it from.

use yunta_testkit::{stderr, stdout, yunta_in};

/// Where the repository keeps the JSON Schemas the binary embeds,
/// relative to this crate. Named once so moving them is one edit.
const COMMITTED_SCHEMAS: &str = "../core/schemas";

#[test]
fn with_no_arguments_it_lists_every_document_yunta_reads() {
    let here = tempfile::tempdir().unwrap();
    let output = yunta_in!(here.path(), here.path(), &["schema"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    for kind in ["tasks", "findings", "questions"] {
        assert!(text.contains(kind), "{kind} missing from {text}");
    }
}

#[test]
fn a_kind_prints_the_shape_a_writer_copies() {
    let here = tempfile::tempdir().unwrap();
    let output = yunta_in!(here.path(), here.path(), &["schema", "tasks"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    // Every required key, and the rule a writer cannot infer.
    for needle in ["tasks:", "id:", "title:", "scope:", "criteria:", "cmd:"] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    // And every rule the engine holds a tasks document to — asserted against the
    // list that enforces them, so this cannot pass on wording that drifted.
    for rule in yunta_core::shape::rules(yunta_core::ArtifactKind::Tasks) {
        let demand = yunta_core::text::one_line(rule.demand);
        assert!(
            text.contains(&demand),
            "`{}` never reaches the writer: {text}",
            rule.code
        );
    }
}

#[test]
fn the_shape_needs_no_project_around_it() {
    // An empty directory: no `.yunta/`, no config, no run. Learning the
    // format cannot depend on having set anything up.
    let here = tempfile::tempdir().unwrap();
    for kind in ["tasks", "findings", "questions"] {
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
    let output = yunta_in!(here.path(), here.path(), &["schema", "plan"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("`plan`"), "{text}");
    assert!(text.contains("`tasks`"), "{text}");
}

/// The JSON Schema the binary serves is the one CI proved matches the
/// types, not one it re-derives. `cargo xtask schema --check` is what
/// keeps the two the same file; this only proves the binary reads it.
#[test]
fn the_json_schema_served_is_the_one_committed_in_the_repository() {
    let here = tempfile::tempdir().unwrap();
    for (kind, file) in [
        ("tasks", "tasks.json"),
        ("findings", "findings.json"),
        ("questions", "questions.json"),
    ] {
        let output = yunta_in!(here.path(), here.path(), &["schema", kind, "--json"]);
        assert!(output.status.success(), "{kind}: {}", stderr(&output));
        let committed = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(COMMITTED_SCHEMAS)
                .join(file),
        )
        .unwrap();
        assert_eq!(stdout(&output), committed, "{kind}");
    }
}

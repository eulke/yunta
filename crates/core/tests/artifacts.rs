//! The tasks document is named `tasks` at every door that reads a kind
//! from text — a workflow's `kind:`, a stored event's `artifact_kind`,
//! the argument of `yunta schema` — and each of those doors still reads
//! the spelling `task-ledger` that earlier workflows and logs carry.

use yunta_core::events::{ArtifactId, ArtifactWrittenPayload};
use yunta_core::{
    ArtifactContextRef, ArtifactKind, ArtifactRefId, ArtifactSpec, Artifacts, ContextSpec, NodeId,
};

#[test]
fn a_workflow_declares_the_tasks_document_as_tasks() {
    let spec: ArtifactSpec =
        yunta_core::yaml::parse("tasks").expect("`tasks` is a kind a workflow declares");
    assert_eq!(spec, ArtifactSpec::Interpreted(ArtifactKind::Tasks));
    assert_eq!(spec.kind(), Some(ArtifactKind::Tasks));
}

#[test]
fn a_workflow_written_with_task_ledger_declares_the_same_kind() {
    let spec: ArtifactSpec = yunta_core::yaml::parse("task-ledger").expect("`task-ledger` reads");
    assert_eq!(spec.kind(), Some(ArtifactKind::Tasks));
}

/// A node declares what it produces as a list of bare strings: one that
/// names a kind is that kind, and every other is a file name.
#[test]
fn produces_reads_a_kind_as_a_kind_and_anything_else_as_a_file_name() {
    let artifacts: Artifacts =
        yunta_core::yaml::parse("produces: [tasks, notes.md]").expect("both forms read");
    assert_eq!(
        artifacts.produces,
        vec![
            ArtifactSpec::Interpreted(ArtifactKind::Tasks),
            ArtifactSpec::Opaque("notes.md".to_string()),
        ]
    );
    assert_eq!(artifacts.produces[1].kind(), None);
}

/// The identity of a declaration is what the log answers by: the kind
/// for an interpreted document, the name for an opaque file.
#[test]
fn a_declaration_is_the_identity_the_log_answers_by() {
    assert_eq!(
        ArtifactId::from(&ArtifactSpec::Interpreted(ArtifactKind::Findings)),
        ArtifactId::Interpreted {
            kind: ArtifactKind::Findings
        }
    );
    assert_eq!(
        ArtifactId::from(&ArtifactSpec::Opaque("brief.md".to_string())),
        ArtifactId::Opaque {
            name: "brief.md".to_string()
        }
    );
}

/// The pair a name and a kind used to make is gone: an author writes one
/// string, and the error says so rather than leaving a mapping half-read.
#[test]
fn a_name_and_kind_pair_is_refused_naming_the_form_that_replaces_it() {
    let error =
        yunta_core::yaml::parse::<Artifacts>("produces: [{ name: plan.yaml, kind: tasks }]")
            .expect_err("a mapping is no longer an artifact");
    let text = error.to_string();
    assert!(
        text.contains("a mapping") && text.contains("tasks"),
        "the error says what was written and lists the kinds: {text}"
    );
}

/// A reference names the artifact by what identifies it: the kind of a
/// document the engine reads, the file name of an opaque one.
#[test]
fn an_artifact_reference_names_a_kind_or_a_name() {
    let by_kind: ArtifactContextRef =
        yunta_core::yaml::parse("{ node: plan, kind: tasks }").expect("a kind reference reads");
    assert_eq!(by_kind.node.as_ref().map(NodeId::as_str), Some("plan"));
    assert_eq!(
        by_kind.id,
        ArtifactRefId::Kind {
            kind: ArtifactKind::Tasks
        }
    );

    let by_name: ArtifactContextRef =
        yunta_core::yaml::parse("{ node: grill, name: brief.md }").expect("a name reference reads");
    assert_eq!(by_name.node.as_ref().map(NodeId::as_str), Some("grill"));
    assert_eq!(
        by_name.id,
        ArtifactRefId::Name {
            name: "brief.md".to_string()
        }
    );
}

/// One or the other, never both and never neither: an artifact is a
/// document of a kind or a file of a name.
#[test]
fn an_artifact_reference_that_names_neither_or_both_is_refused() {
    for (yaml, why) in [
        ("{ node: plan }", "neither"),
        ("{ node: plan, kind: tasks, name: plan.yaml }", "both"),
        (
            "{ node: plan, knd: tasks }",
            "a key the schema does not have",
        ),
    ] {
        let error = yunta_core::yaml::parse::<ArtifactContextRef>(yaml)
            .err()
            .unwrap_or_else(|| panic!("`{yaml}` names {why} and must be refused"));
        let text = error.to_string();
        assert!(
            text.contains("kind") && text.contains("name"),
            "the refusal of {why} says what a reference names: {text}"
        );
    }
}

#[test]
fn the_tasks_kind_serializes_as_tasks() {
    assert_eq!(
        serde_json::to_value(ArtifactKind::Tasks).expect("a kind serializes"),
        serde_json::json!("tasks")
    );
    assert_eq!(ArtifactKind::Tasks.as_str(), "tasks");
}

#[test]
fn a_schema_argument_spelled_task_ledger_names_the_tasks_kind() {
    assert_eq!(
        "task-ledger"
            .parse::<ArtifactKind>()
            .expect("the alias reads"),
        ArtifactKind::Tasks
    );
    assert_eq!(
        "tasks".parse::<ArtifactKind>().expect("the name reads"),
        ArtifactKind::Tasks
    );
}

#[test]
fn a_stored_artifact_written_with_task_ledger_deserializes() {
    let payload: ArtifactWrittenPayload = serde_json::from_str(
        r#"{"path":"artifacts/plan.yaml","content_hash":"0000000000000000000000000000000000000000000000000000000000000000","artifact_kind":"task-ledger"}"#,
    )
    .expect("an earlier log's payload reads");
    assert_eq!(payload.artifact_kind, Some(ArtifactKind::Tasks));
}

#[test]
fn the_tasks_context_source_reads_under_both_spellings_and_writes_tasks() {
    let current: ContextSpec =
        yunta_core::yaml::parse("tasks: {}").expect("`tasks:` is a context source");
    let earlier: ContextSpec =
        yunta_core::yaml::parse("ledger: {}").expect("`ledger:` still reads");
    assert!(matches!(current, ContextSpec::Tasks { .. }), "{current:?}");
    assert_eq!(current, earlier);
    assert_eq!(
        serde_json::to_value(&current).expect("a context source serializes"),
        serde_json::json!({ "tasks": {} })
    );
}

#[test]
fn an_artifact_name_with_a_parent_segment_is_refused() {
    for climbing in ["../escaped.md", "out/../../escaped.md", "..", "/etc/passwd"] {
        assert!(
            yunta_core::ArtifactName::parse(climbing).is_err(),
            "`{climbing}` leaves the run directory",
        );
    }
    assert_eq!(
        yunta_core::ArtifactName::parse("report.md")
            .expect("a plain file name is a name")
            .as_str(),
        "report.md",
    );
    assert_eq!(
        yunta_core::ArtifactName::parse("out/report.md")
            .expect("a name may sit in a directory of its own")
            .as_str(),
        "out/report.md",
    );
}

#[test]
fn an_artifact_name_that_claims_what_the_engine_writes_is_refused() {
    for identity in yunta_core::ReservedIdentity::all() {
        let claimed = identity.file_name();
        assert!(
            yunta_core::ArtifactName::parse(&claimed).is_err(),
            "`{claimed}` is what the run's view writes for {identity}",
        );
    }
    assert!(yunta_core::ArtifactName::parse("tasks.yml").is_ok());
}

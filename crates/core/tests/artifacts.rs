//! The tasks document is named `tasks` at every door that reads a kind
//! from text — a workflow's `kind:`, a stored event's `artifact_kind`,
//! the argument of `yunta schema` — and each of those doors still reads
//! the spelling `task-ledger` that earlier workflows and logs carry.

use yunta_core::events::ArtifactWrittenPayload;
use yunta_core::{ArtifactKind, ArtifactSpec, ContextSpec};

#[test]
fn a_workflow_declares_the_tasks_document_as_tasks() {
    let spec: ArtifactSpec = yunta_core::yaml::parse("{ name: plan.yaml, kind: tasks }")
        .expect("`tasks` is a kind a workflow declares");
    assert_eq!(spec.kind(), Some(ArtifactKind::Tasks));
}

#[test]
fn a_workflow_written_with_task_ledger_declares_the_same_kind() {
    let spec: ArtifactSpec = yunta_core::yaml::parse("{ name: plan.yaml, kind: task-ledger }")
        .expect("`task-ledger` still reads");
    assert_eq!(spec.kind(), Some(ArtifactKind::Tasks));
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

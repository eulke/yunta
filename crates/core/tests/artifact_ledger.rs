//! What the fold from artifact events promises: an artifact is
//! identified by what it is rather than by where it was written, the
//! last acceptance of one identity is what the run holds, and a log
//! written before `artifact_accepted` existed still names its artifacts.

use proptest::prelude::*;
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, ArtifactOrigin, ArtifactWrittenPayload, EventBody,
    EventPayload, RunPausedPayload, StoredEvent,
};
use yunta_core::{sha256_hex, ArtifactKind, ContentHash, NodeId, RunId};

fn hash(content: &str) -> ContentHash {
    sha256_hex(content.as_bytes())
}

fn event(seq: u64, node: Option<&str>, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        seq: seq.into(),
        run_id: RunId::from("run-1"),
        node_id: node.map(NodeId::from),
        timestamp: chrono::DateTime::UNIX_EPOCH,
        body: EventBody::Known(payload),
    }
}

fn accepted(seq: u64, node: Option<&str>, artifact: ArtifactId, content: &str) -> StoredEvent {
    event(
        seq,
        node,
        EventPayload::ArtifactAccepted(ArtifactAcceptedPayload {
            artifact,
            content_hash: hash(content),
            origin: ArtifactOrigin::Submitted,
        }),
    )
}

fn interpreted(kind: ArtifactKind) -> ArtifactId {
    ArtifactId::Interpreted { kind }
}

fn opaque(name: &str) -> ArtifactId {
    ArtifactId::Opaque {
        name: name.to_string(),
    }
}

fn written(seq: u64, node: &str, path: &str, kind: Option<ArtifactKind>) -> StoredEvent {
    event(
        seq,
        Some(node),
        EventPayload::ArtifactWritten(ArtifactWrittenPayload {
            path: path.into(),
            content_hash: hash(path),
            artifact_kind: kind,
        }),
    )
}

#[test]
fn the_last_acceptance_of_one_identity_is_what_the_run_holds() {
    let log = vec![
        accepted(1, Some("plan"), interpreted(ArtifactKind::Tasks), "first"),
        accepted(2, Some("plan"), opaque("notes.md"), "notes"),
        accepted(3, Some("plan"), interpreted(ArtifactKind::Tasks), "second"),
    ];
    let ledger = ArtifactLedger::of(&log);

    let tasks = ledger
        .latest(
            &interpreted(ArtifactKind::Tasks),
            Some(&NodeId::from("plan")),
        )
        .expect("the tasks document stands");
    assert_eq!(tasks.content_hash, hash("second"));
    assert_eq!(tasks.seq, 3u64.into());

    let order: Vec<ArtifactId> = ledger.every().map(|r| r.artifact.clone()).collect();
    assert_eq!(
        order,
        vec![interpreted(ArtifactKind::Tasks), opaque("notes.md")],
        "a second acceptance replaces the content and moves nothing"
    );
}

#[test]
fn the_identity_alone_reaches_the_last_producer_of_it() {
    let log = vec![
        accepted(1, Some("plan"), interpreted(ArtifactKind::Tasks), "plan's"),
        accepted(2, Some("fix"), interpreted(ArtifactKind::Tasks), "fix's"),
    ];
    let ledger = ArtifactLedger::of(&log);

    let latest = ledger
        .latest(&interpreted(ArtifactKind::Tasks), None)
        .expect("some node produced a tasks document");
    assert_eq!(latest.producer, Some(NodeId::from("fix")));
    assert_eq!(latest.content_hash, hash("fix's"));
}

#[test]
fn a_named_producer_never_reaches_another_nodes_artifact() {
    let log = vec![accepted(
        1,
        Some("fix"),
        interpreted(ArtifactKind::Findings),
        "fix's",
    )];
    let ledger = ArtifactLedger::of(&log);

    assert!(
        ledger
            .latest(
                &interpreted(ArtifactKind::Findings),
                Some(&NodeId::from("plan"))
            )
            .is_none(),
        "`plan` produced no findings artifact"
    );
    assert!(ledger
        .latest(
            &interpreted(ArtifactKind::Findings),
            Some(&NodeId::from("fix"))
        )
        .is_some());
}

#[test]
fn a_written_artifact_with_a_kind_folds_as_that_interpreted_identity() {
    let log = vec![written(
        1,
        "plan",
        "artifacts/plan.yaml",
        Some(ArtifactKind::Tasks),
    )];
    let ledger = ArtifactLedger::of(&log);

    let stood = ledger
        .latest(&interpreted(ArtifactKind::Tasks), None)
        .expect("an earlier log still names its tasks document");
    assert_eq!(stood.content_hash, hash("artifacts/plan.yaml"));
    assert_eq!(stood.origin, ArtifactOrigin::Legacy);
}

#[test]
fn a_written_artifact_without_a_kind_folds_as_its_name_under_artifacts() {
    let log = vec![written(1, "plan", "artifacts/notes.md", None)];
    let ledger = ArtifactLedger::of(&log);

    assert!(
        ledger.latest(&opaque("notes.md"), None).is_some(),
        "the identity is the name, not the path it was written at"
    );
}

#[test]
fn a_written_artifact_under_a_subdirectory_keeps_the_whole_name() {
    let log = vec![written(1, "plan", "artifacts/sub/dir/x.md", None)];
    let ledger = ArtifactLedger::of(&log);

    assert!(ledger.latest(&opaque("sub/dir/x.md"), None).is_some());
}

#[test]
fn a_kind_and_a_producer_each_select_their_own_refs_in_order() {
    let log = vec![
        accepted(1, Some("plan"), interpreted(ArtifactKind::Tasks), "t"),
        accepted(2, Some("plan"), opaque("notes.md"), "n"),
        accepted(3, Some("review"), interpreted(ArtifactKind::Findings), "f"),
        accepted(4, Some("review"), interpreted(ArtifactKind::Tasks), "t2"),
    ];
    let ledger = ArtifactLedger::of(&log);

    let of_tasks: Vec<Option<NodeId>> = ledger
        .of_kind(ArtifactKind::Tasks)
        .map(|r| r.producer.clone())
        .collect();
    assert_eq!(
        of_tasks,
        vec![Some(NodeId::from("plan")), Some(NodeId::from("review"))],
        "both producers' tasks documents, in first-acceptance order"
    );

    let by_plan: Vec<ArtifactId> = ledger
        .by_producer(&NodeId::from("plan"))
        .map(|r| r.artifact.clone())
        .collect();
    assert_eq!(
        by_plan,
        vec![interpreted(ArtifactKind::Tasks), opaque("notes.md")]
    );
}

#[test]
fn an_event_about_something_else_leaves_the_fold_unmoved() {
    let mut ledger = ArtifactLedger::default();
    ledger.apply(
        None,
        1u64.into(),
        &EventPayload::RunPaused(RunPausedPayload {
            reason: "gate waiting".to_string(),
        }),
    );
    assert_eq!(ledger.every().count(), 0);
}

#[test]
fn an_artifact_the_run_acquires_without_a_node_stands_like_any_other() {
    let log = vec![accepted(
        1,
        None,
        opaque("brief.md"),
        "what the run was given",
    )];
    let ledger = ArtifactLedger::of(&log);

    let stood = ledger
        .latest(&opaque("brief.md"), None)
        .expect("an input document stands");
    assert_eq!(stood.producer, None);
}

proptest! {
    /// The fold is deterministic, and every identity a log accepts is
    /// reachable through `every` — nothing the log stated is dropped.
    #[test]
    fn the_fold_is_deterministic_and_loses_no_identity(
        steps in prop::collection::vec((0usize..3, 0usize..3), 0..24)
    ) {
        let producers = ["plan", "review", "fix"];
        let ids = [
            interpreted(ArtifactKind::Tasks),
            interpreted(ArtifactKind::Findings),
            opaque("notes.md"),
        ];
        let log: Vec<StoredEvent> = steps
            .iter()
            .enumerate()
            .map(|(seq, (producer, id))| {
                accepted(
                    seq as u64 + 1,
                    Some(producers[*producer]),
                    ids[*id].clone(),
                    &seq.to_string(),
                )
            })
            .collect();

        let ledger = ArtifactLedger::of(&log);
        prop_assert_eq!(&ledger, &ArtifactLedger::of(&log));

        let held: std::collections::BTreeSet<(Option<NodeId>, ArtifactId)> = ledger
            .every()
            .map(|r| (r.producer.clone(), r.artifact.clone()))
            .collect();
        let stated: std::collections::BTreeSet<(Option<NodeId>, ArtifactId)> = steps
            .iter()
            .map(|(producer, id)| {
                (Some(NodeId::from(producers[*producer])), ids[*id].clone())
            })
            .collect();
        prop_assert_eq!(held, stated);
    }
}

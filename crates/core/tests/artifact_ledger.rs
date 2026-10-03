//! What the fold from artifact events promises: an artifact is
//! identified by what it is rather than by where it was written, the
//! last acceptance of one identity is what the run holds, and a log
//! written before `artifact_accepted` existed still names its artifacts.

use proptest::prelude::*;
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::ArtifactEvent;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, ArtifactOrigin, ArtifactSubmittedPayload,
    ArtifactWrittenPayload, EventPayload, RecordedOrigin, StoredEvent, SubmissionOutcome,
};
use yunta_core::{sha256_hex, ArtifactKind, ContentHash, NodeId};
use yunta_testkit_core::Log;

fn hash(content: &str) -> ContentHash {
    sha256_hex(content.as_bytes())
}

fn accepted(artifact: ArtifactId, content: &str) -> EventPayload {
    EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
        artifact,
        hash(content),
        RecordedOrigin::Submitted,
    )))
}

fn interpreted(kind: ArtifactKind) -> ArtifactId {
    ArtifactId::Interpreted { kind }
}

fn opaque(name: &str) -> ArtifactId {
    ArtifactId::Opaque {
        name: name.to_string(),
    }
}

fn written(path: &str, kind: Option<ArtifactKind>) -> EventPayload {
    EventPayload::Artifacts(ArtifactEvent::Written(ArtifactWrittenPayload {
        path: path.into(),
        content_hash: hash(path),
        artifact_kind: kind,
    }))
}

#[test]
fn the_last_acceptance_of_one_identity_is_what_the_run_holds() {
    let log = Log::for_run("run-1")
        .node("plan", accepted(interpreted(ArtifactKind::Tasks), "first"))
        .node("plan", accepted(opaque("notes.md"), "notes"))
        .node("plan", accepted(interpreted(ArtifactKind::Tasks), "second"))
        .build();
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
    let log = Log::for_run("run-1")
        .node("plan", accepted(interpreted(ArtifactKind::Tasks), "plan's"))
        .node("fix", accepted(interpreted(ArtifactKind::Tasks), "fix's"))
        .build();
    let ledger = ArtifactLedger::of(&log);

    let latest = ledger
        .latest(&interpreted(ArtifactKind::Tasks), None)
        .expect("some node produced a tasks document");
    assert_eq!(latest.producer, Some(NodeId::from("fix")));
    assert_eq!(latest.content_hash, hash("fix's"));
}

#[test]
fn a_named_producer_never_reaches_another_nodes_artifact() {
    let log = Log::for_run("run-1")
        .node(
            "fix",
            accepted(interpreted(ArtifactKind::Findings), "fix's"),
        )
        .build();
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
    let log = Log::for_run("run-1")
        .node(
            "plan",
            written("artifacts/plan.yaml", Some(ArtifactKind::Tasks)),
        )
        .build();
    let ledger = ArtifactLedger::of(&log);

    let stood = ledger
        .latest(&interpreted(ArtifactKind::Tasks), None)
        .expect("an earlier log still names its tasks document");
    assert_eq!(stood.content_hash, hash("artifacts/plan.yaml"));
    assert!(stood.origin == ArtifactOrigin::Legacy);
}

#[test]
fn a_written_artifact_without_a_kind_folds_as_its_name_under_artifacts() {
    let log = Log::for_run("run-1")
        .node("plan", written("artifacts/notes.md", None))
        .build();
    let ledger = ArtifactLedger::of(&log);

    assert!(
        ledger.latest(&opaque("notes.md"), None).is_some(),
        "the identity is the name, not the path it was written at"
    );
}

#[test]
fn a_written_artifact_under_a_subdirectory_keeps_the_whole_name() {
    let log = Log::for_run("run-1")
        .node("plan", written("artifacts/sub/dir/x.md", None))
        .build();
    let ledger = ArtifactLedger::of(&log);

    assert!(ledger.latest(&opaque("sub/dir/x.md"), None).is_some());
}

#[test]
fn a_kind_and_a_producer_each_select_their_own_refs_in_order() {
    let log = Log::for_run("run-1")
        .node("plan", accepted(interpreted(ArtifactKind::Tasks), "t"))
        .node("plan", accepted(opaque("notes.md"), "n"))
        .node("review", accepted(interpreted(ArtifactKind::Findings), "f"))
        .node("review", accepted(interpreted(ArtifactKind::Tasks), "t2"))
        .build();
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
fn a_submission_leaves_the_fold_unmoved() {
    // What the run holds is what the engine accepted: a session handing
    // something over states nothing about that until the acceptance
    // follows, and an event of another domain cannot reach this fold at
    // all — the type says so.
    let mut ledger = ArtifactLedger::default();
    ledger.apply(
        None,
        1u64.into(),
        &ArtifactEvent::Submitted(ArtifactSubmittedPayload {
            name: "plan.md".to_string(),
            artifact_kind: yunta_core::ArtifactKind::Tasks,
            outcome: SubmissionOutcome::Accepted {
                content_hash: yunta_core::sha256_hex(b"plan"),
            },
        }),
    );
    assert_eq!(ledger.every().count(), 0);
}

#[test]
fn an_artifact_the_run_acquires_without_a_node_stands_like_any_other() {
    let log = Log::for_run("run-1")
        .event(accepted(opaque("brief.md"), "what the run was given"))
        .build();
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
            .fold(Log::for_run("run-1"), |log, (seq, (producer, id))| {
                log.node(
                    producers[*producer],
                    accepted(ids[*id].clone(), &seq.to_string()),
                )
            })
            .build();

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

/// Only a fold over a log written before origins were recorded produces
/// `Legacy`. Everything a run records today says where the artifact came
/// from, and the type is what makes the other answer unreachable: a
/// constructor takes a [`RecordedOrigin`], which has no `Legacy` arm.
#[test]
fn a_fresh_acceptance_cannot_be_legacy() {
    let accepted = ArtifactAcceptedPayload::new(
        interpreted(ArtifactKind::Findings),
        yunta_core::sha256_hex(b"findings"),
        RecordedOrigin::Derived,
    );

    assert_eq!(accepted.origin, RecordedOrigin::Derived);
}

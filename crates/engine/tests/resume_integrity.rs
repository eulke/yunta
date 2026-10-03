//! A resume verifies that the run still holds the bytes its log accepted.
//!
//! Waking a run is the moment to ask whether its artifacts are still the
//! artifacts: every acceptance on the log names an object, and a resume
//! reads each one before it says the run resumed. An object that is gone,
//! or whose content no longer hashes to its own name, leaves the run
//! `broken` with a diagnostic — never a run that keeps handing readers
//! bytes its own history never saw. The `artifacts/` view is derived and
//! is not part of that question.

use std::path::{Path, PathBuf};

use yunta_core::events::{
    ArtifactEvent, ArtifactId, ArtifactWrittenPayload, EventDraft, EventPayload, NodeEvent,
    NodeStartedPayload, RunEvent,
};
use yunta_core::{sha256_hex, ContentHash, SystemClock};
use yunta_engine::{BirthArtifact, BirthOrigin, RunError, RunReport, RunTerminal};
use yunta_testkit::Bench;

/// One node the engine finds interrupted with no terminal event, whose
/// policy is to pause rather than guess — so every invocation of this run
/// is a resume that stops again, and a test can wake it as often as it
/// needs to.
const ONE_INTERRUPTED_NODE: &str = r#"
name: holds-an-artifact
nodes:
  - id: only
    kind: bash
    on_interrupt: fail_if_uncertain
    run: "true"
"#;

/// The run's one node runs a command, so no session is scripted.
const NO_SESSIONS: &str = "sessions: []";

/// The artifact the run is born holding.
const REPORT: &[u8] = b"what the run was handed at birth";

/// What a predecessor handed the run over: `report.md`, accepted at
/// birth, whose object every resume verifies.
fn inherited_report() -> BirthArtifact {
    BirthArtifact {
        artifact: ArtifactId::Opaque {
            name: "report.md".to_string(),
        },
        origin: BirthOrigin::Inherited {
            run: "run-predecessor".into(),
            producer: None,
        },
        bytes: REPORT.to_vec(),
    }
}

/// A run born holding that artifact, with nothing executed yet.
fn born_holding_report() -> Bench {
    Bench::new().born_holding(vec![inherited_report()])
}

/// A crash mid-node: `node_started` with no terminal event, under the
/// one policy that pauses instead of re-running.
fn interrupt(bench: &Bench) {
    bench
        .storage
        .append(
            &EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            },
            &SystemClock,
        )
        .unwrap();
}

/// Where the run keeps the bytes an acceptance names.
fn object(run_dir: &Path, hash: &ContentHash) -> PathBuf {
    run_dir.join("objects").join(hash.as_str())
}

/// The `run_resumed` payloads on a run's log, one per invocation that
/// woke it.
fn resumes(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| matches!(e.payload(), Some(EventPayload::Run(RunEvent::Resumed(_)))))
        .count()
}

#[tokio::test]
async fn a_resume_over_an_intact_store_records_its_resume_and_nothing_before_it() {
    let bench = born_holding_report();

    let RunReport { terminal, .. } = bench
        .run_sabotaged(ONE_INTERRUPTED_NODE, NO_SESSIONS, |_| interrupt(&bench))
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the interrupted node's policy still decides the outcome: {terminal:?}"
    );
    let events = bench.events();
    let kinds: Vec<&str> = events.iter().map(|e| e.body.kind_name()).collect();
    assert_eq!(
        kinds,
        vec![
            "run_created",
            "artifact_accepted",
            "node_started",
            "run_resumed",
            "run_paused",
        ],
        "verifying an intact store adds nothing to the log: {kinds:?}"
    );
}

#[tokio::test]
async fn a_resume_whose_object_was_replaced_marks_the_run_broken() {
    let bench = born_holding_report();
    let hash = sha256_hex(REPORT);

    let error = bench
        .try_run_sabotaged(ONE_INTERRUPTED_NODE, NO_SESSIONS, |run_dir| {
            interrupt(&bench);
            std::fs::write(object(run_dir, &hash), b"not what the run accepted").unwrap();
        })
        .await
        .expect_err("the run no longer holds what its log names");

    let RunError::Broken { diagnostic } = &error else {
        panic!("a run whose artifact is not itself is broken: {error:?}");
    };
    assert!(
        diagnostic.contains("report.md"),
        "the diagnostic names the artifact: {diagnostic}"
    );
    assert!(
        diagnostic.contains(bench.run_id.as_str()),
        "the diagnostic names the run: {diagnostic}"
    );
    assert!(
        diagnostic.contains(hash.as_str()),
        "the diagnostic names the hash the log accepted: {diagnostic}"
    );
    assert!(
        diagnostic.contains(sha256_hex(b"not what the run accepted").as_str()),
        "the diagnostic names what the bytes hash to now: {diagnostic}"
    );
    assert_eq!(
        resumes(&bench),
        0,
        "a run that cannot be verified never records that it resumed"
    );
}

#[tokio::test]
async fn a_resume_whose_object_is_gone_marks_the_run_broken() {
    let bench = born_holding_report();
    let hash = sha256_hex(REPORT);

    let error = bench
        .try_run_sabotaged(ONE_INTERRUPTED_NODE, NO_SESSIONS, |run_dir| {
            interrupt(&bench);
            std::fs::remove_file(object(run_dir, &hash)).unwrap();
        })
        .await
        .expect_err("the bytes the log names are not there");

    let RunError::Broken { diagnostic } = &error else {
        panic!("a run missing an artifact's bytes is broken: {error:?}");
    };
    assert!(
        diagnostic.contains("report.md") && diagnostic.contains(hash.as_str()),
        "the diagnostic names the artifact and the object: {diagnostic}"
    );
    assert!(
        diagnostic.contains("no object"),
        "the diagnostic says the bytes are missing, not that they differ: {diagnostic}"
    );
}

#[tokio::test]
async fn deleting_the_view_of_an_artifact_leaves_the_resume_alone() {
    let bench = born_holding_report();

    let RunReport { terminal, .. } = bench
        .run_sabotaged(ONE_INTERRUPTED_NODE, NO_SESSIONS, |run_dir| {
            interrupt(&bench);
            std::fs::remove_dir_all(run_dir.join("artifacts")).unwrap();
        })
        .await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&bench), 1, "the run woke normally");
}

#[tokio::test]
async fn a_run_whose_log_predates_the_object_store_resumes_and_says_what_it_could_not_check() {
    let bench = born_holding_report();

    let RunReport { terminal, state } = bench
        .run_sabotaged(ONE_INTERRUPTED_NODE, NO_SESSIONS, |_| {
            interrupt(&bench);
            // What a log written before the object store says about an
            // artifact: an `artifact_written` naming a file, with no
            // object behind it.
            bench
                .storage
                .append(
                    &EventDraft {
                        run_id: bench.run_id.clone(),
                        node_id: Some("only".into()),
                        payload: EventPayload::Artifacts(ArtifactEvent::Written(
                            ArtifactWrittenPayload {
                                path: PathBuf::from("artifacts/legacy.md"),
                                content_hash: sha256_hex(b"bytes this run never stored"),
                                artifact_kind: None,
                            },
                        )),
                    },
                    &SystemClock,
                )
                .unwrap();
        })
        .await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    let standing = state.effective_findings();
    let finding = standing
        .iter()
        .find(|finding| finding.detail.contains("artifact_written"))
        .unwrap_or_else(|| {
            panic!(
                "the resume says what it could not verify: {:?}",
                state.effective_findings()
            )
        });
    assert!(
        finding.detail.contains('1'),
        "it says how many artifacts it left unchecked: {}",
        finding.detail
    );
}

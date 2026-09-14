//! A resume verifies that the run still holds the bytes its log accepted.
//!
//! Waking a run is the moment to ask whether its artifacts are still the
//! artifacts: every acceptance on the log names an object, and a resume
//! reads each one before it says the run resumed. An object that is gone,
//! or whose content no longer hashes to its own name, leaves the run
//! `broken` with a diagnostic — never a run that keeps handing readers
//! bytes its own history never saw. The `artifacts/` view is derived and
//! is not part of that question.

use std::collections::HashMap;
use std::path::PathBuf;

use yunta_core::events::{
    ArtifactId, ArtifactWrittenPayload, EventDraft, EventPayload, NodeStartedPayload,
};
use yunta_core::{sha256_hex, ConfigLayer, ContentHash, Manifest, SystemClock, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, BirthArtifact, BirthOrigin, CreateRunParams,
    NoInteraction, RunEnv, RunError, RunReport, RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, MOCK_CONFIG};
use yunta_testkit_core::FixedClock;

mod common;
use common::*;
use yunta_core::events::{ArtifactEvent, NodeEvent, RunEvent};

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

/// The artifact the run is born holding.
const REPORT: &[u8] = b"what the run was handed at birth";

/// A run paused mid-node that holds one artifact: the bytes are in its
/// object store and the acceptance is on its log, which is the state a
/// resume verifies.
struct Paused {
    bench: Bench,
    manifest: Manifest,
    run_dir: PathBuf,
    hash: ContentHash,
}

impl Paused {
    /// Wakes the run again — the resume under test.
    async fn resume(&self) -> Result<RunReport, RunError> {
        execute_run(RunEnv {
            run_id: &self.bench.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &self.bench.worktree,
            adapters: &HashMap::new(),
            storage: &self.bench.storage.async_handle(),
            clock: std::sync::Arc::new(FixedClock),
            ids: &IDS,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &NoInteraction,
            forge: None,
            cancel: None,
            adapter_override: None,
            ambient: None,
            observer: None,
        })
        .await
    }

    /// Where the bytes of the run's one artifact live.
    fn object(&self) -> PathBuf {
        self.run_dir.join("objects").join(self.hash.as_str())
    }
}

/// Builds that run: born holding `report.md`, then interrupted mid-node.
async fn paused_run() -> Paused {
    let bench = Bench::new();
    let workflow: Workflow = serde_norway::from_str(ONE_INTERRUPTED_NODE).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap()
    .manifest;
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            worktree: &bench.worktree,
            promoted_from: None,
            artifacts: &[BirthArtifact {
                artifact: ArtifactId::Opaque {
                    name: "report.md".to_string(),
                },
                origin: BirthOrigin::Inherited {
                    run: "run-predecessor".into(),
                    producer: None,
                },
                bytes: REPORT.to_vec(),
            }],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    // A crash mid-node: `node_started` with no terminal event, under the
    // one policy that pauses instead of re-running.
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

    Paused {
        manifest,
        run_dir,
        hash: sha256_hex(REPORT),
        bench,
    }
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
    let paused = paused_run().await;

    let report = paused
        .resume()
        .await
        .expect("the run wakes and pauses again");

    assert!(
        matches!(report.terminal, RunTerminal::Paused { .. }),
        "the interrupted node's policy still decides the outcome: {:?}",
        report.terminal
    );
    let events = paused.bench.events();
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
    let paused = paused_run().await;
    std::fs::write(paused.object(), b"not what the run accepted").unwrap();

    let error = paused
        .resume()
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
        diagnostic.contains(paused.bench.run_id.as_str()),
        "the diagnostic names the run: {diagnostic}"
    );
    assert!(
        diagnostic.contains(paused.hash.as_str()),
        "the diagnostic names the hash the log accepted: {diagnostic}"
    );
    assert!(
        diagnostic.contains(sha256_hex(b"not what the run accepted").as_str()),
        "the diagnostic names what the bytes hash to now: {diagnostic}"
    );
    assert_eq!(
        resumes(&paused.bench),
        0,
        "a run that cannot be verified never records that it resumed"
    );
}

#[tokio::test]
async fn a_resume_whose_object_is_gone_marks_the_run_broken() {
    let paused = paused_run().await;
    std::fs::remove_file(paused.object()).unwrap();

    let error = paused
        .resume()
        .await
        .expect_err("the bytes the log names are not there");

    let RunError::Broken { diagnostic } = &error else {
        panic!("a run missing an artifact's bytes is broken: {error:?}");
    };
    assert!(
        diagnostic.contains("report.md") && diagnostic.contains(paused.hash.as_str()),
        "the diagnostic names the artifact and the object: {diagnostic}"
    );
    assert!(
        diagnostic.contains("no object"),
        "the diagnostic says the bytes are missing, not that they differ: {diagnostic}"
    );
}

#[tokio::test]
async fn deleting_the_view_of_an_artifact_leaves_the_resume_alone() {
    let paused = paused_run().await;
    std::fs::remove_dir_all(paused.run_dir.join("artifacts")).unwrap();

    let report = paused
        .resume()
        .await
        .expect("the view is derived: deleting it says nothing about what the run holds");

    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&paused.bench), 1, "the run woke normally");
}

#[tokio::test]
async fn a_run_whose_log_predates_the_object_store_resumes_and_says_what_it_could_not_check() {
    let paused = paused_run().await;
    // What a log written before the object store says about an artifact:
    // an `artifact_written` naming a file, with no object behind it.
    paused
        .bench
        .storage
        .append(
            &EventDraft {
                run_id: paused.bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: EventPayload::Artifacts(ArtifactEvent::Written(ArtifactWrittenPayload {
                    path: PathBuf::from("artifacts/legacy.md"),
                    content_hash: sha256_hex(b"bytes this run never stored"),
                    artifact_kind: None,
                })),
            },
            &SystemClock,
        )
        .unwrap();

    let report = paused
        .resume()
        .await
        .expect("an artifact from before the store cannot be checked against one");

    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));
    let finding = report
        .state
        .findings
        .iter()
        .find(|finding| finding.detail.contains("artifact_written"))
        .unwrap_or_else(|| {
            panic!(
                "the resume says what it could not verify: {:?}",
                report.state.findings
            )
        });
    assert!(
        finding.detail.contains('1'),
        "it says how many artifacts it left unchecked: {}",
        finding.detail
    );
}

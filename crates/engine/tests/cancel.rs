//! External cancellation of a run. A `CancellationToken` fired while a node
//! holds a session that never ends on its own interrupts that session,
//! pauses the run as `cancelled by user`, and leaves a log that resumes
//! cleanly — the interrupted node runs again and the run finishes. This is
//! the in-engine half of what `yunta cancel` drives from outside the
//! process; the `hang` mock stands in for a stuck agent.

use std::collections::HashMap;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::events::EventPayload;
use yunta_core::{AdapterId, ConfigLayer, Manifest, SeqIdSource, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv, RunTerminal,
    DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, FixedClock, MOCK_CONFIG};

const HANGING_WORKFLOW: &str = "\
name: cancel-me
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: work
";

/// A single session that opens and then never produces a terminal event on
/// its own — only an interrupt or kill ends it.
const HANG_FIXTURE: &str = "sessions:\n  - { outcome: { type: hang } }\n";

/// A single session that finishes at once — what the interrupted node meets
/// when the run resumes.
const COMPLETING_FIXTURE: &str = "sessions:\n  - { outcome: { type: completed, summary: done } }\n";

/// Builds the manifest and creates the run both tests execute, so they
/// share one setup and differ only in what happens while the run hangs.
async fn prepare(bench: &Bench) -> (Manifest, std::path::PathBuf) {
    let workflow: Workflow = serde_norway::from_str(HANGING_WORKFLOW).expect("parse workflow");
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).expect("parse config");
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .expect("build manifest");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .expect("create run");
    (manifest, run_dir)
}

fn adapters(fixture: &str) -> HashMap<AdapterId, Arc<dyn Adapter>> {
    let mut map: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    map.insert(
        "mock".into(),
        Arc::new(MockAdapter::from_yaml(fixture).expect("parse fixture")),
    );
    map
}

/// Fires `token` as soon as the run's node is under way. A hung run cannot
/// finish on its own, so waiting for `node_started` to land on the log —
/// never a timer — makes the cancellation deterministic.
async fn cancel_once_started(bench: &Bench, token: &CancellationToken) {
    loop {
        let started = bench
            .storage
            .events_for_run(&bench.run_id)
            .expect("read events")
            .iter()
            .any(|event| matches!(event.payload(), Some(EventPayload::NodeStarted(_))));
        if started {
            break;
        }
        tokio::task::yield_now().await;
    }
    token.cancel();
}

#[tokio::test]
async fn cancel_pauses_a_hanging_run_as_cancelled_by_user() {
    let bench = Bench::new();
    let (manifest, run_dir) = prepare(&bench).await;
    let ids = SeqIdSource::new("cancel");
    let adapters = adapters(HANG_FIXTURE);
    let storage = bench.storage.async_handle();
    let token = CancellationToken::new();

    let run = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &storage,
        clock: Arc::new(FixedClock),
        ids: &ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: Some(&token),
        adapter_override: None,
        ambient: None,
        observer: None,
    });

    let (report, ()) = tokio::join!(run, cancel_once_started(&bench, &token));
    let report = report.expect("execute run");
    match report.terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(reason, "cancelled by user");
        }
        other => panic!("a cancelled run must pause, got {other:?}"),
    }
}

#[tokio::test]
async fn cancel_then_resume_finishes_the_run() {
    let bench = Bench::new();
    let (manifest, run_dir) = prepare(&bench).await;
    let ids = SeqIdSource::new("cancel");
    let storage = bench.storage.async_handle();

    // First: cancel the hanging run, exactly as the test above does.
    let hang = adapters(HANG_FIXTURE);
    let token = CancellationToken::new();
    let run = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &hang,
        storage: &storage,
        clock: Arc::new(FixedClock),
        ids: &ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: Some(&token),
        adapter_override: None,
        ambient: None,
        observer: None,
    });
    let (first, ()) = tokio::join!(run, cancel_once_started(&bench, &token));
    assert!(matches!(
        first.expect("execute run").terminal,
        RunTerminal::Paused { .. }
    ));

    // Then: resume with a session that completes. Resume re-derives the log
    // and re-runs the interrupted node, which now finishes the run — no
    // cancellation token this time.
    let completing = adapters(COMPLETING_FIXTURE);
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &completing,
        storage: &storage,
        clock: Arc::new(FixedClock),
        ids: &ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
        observer: None,
    })
    .await
    .expect("resume run");
    assert_eq!(resumed.terminal, RunTerminal::Finished);
}

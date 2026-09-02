//! `kind: gate` with `external: {kind: pull_request}`,
//! exercised end-to-end against `MockForge` — never a real network call
//! (the no-real-LLM-or-network-call-in-tests rule extended to forges). The
//! core scenario: person B approves the PR without Yunta installed at all (simulated by
//! driving `MockForgeState` directly, never through the `Forge` trait —
//! exactly what "no Yunta on B's machine" means), and person A's
//! machine picks up the approval on a completely separate `execute_run`
//! call, simulating `yunta resume`.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockForge, MockForgeState};
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;

/// Run ids for everything a test run gives birth to — unique across
/// the binary, so parallel tests never share a run directory.
static IDS: SeqIdSource = SeqIdSource::new("minted");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

const GATE_ONLY_WORKFLOW: &str = r#"
name: gate-scenario
nodes:
  - id: approve
    kind: gate
    assignee: reviewer
    external:
      kind: pull_request
      artifacts: []
      branch: "{{run.branch}}"
"#;

/// A gate followed by a node that always fails with no `on_failure` —
/// once the gate resolves the run still has unresolved work (`after`
/// pauses rather than finishing), which is what keeps the run's own log
/// free of `run_finished` long enough to exercise a *later* SHA-drift
/// recheck: `execute_run` treats a genuinely finished run as an
/// immutable no-op (reopening anything after `run_finished`
/// would mean mutating a run the log already closed), so the drift
/// recheck only ever matters, and only ever runs, while the run is
/// still open.
const GATE_THEN_UNRESOLVED_WORKFLOW: &str = r#"
name: gate-scenario
nodes:
  - id: approve
    kind: gate
    assignee: reviewer
    external:
      kind: pull_request
      artifacts: []
      branch: "{{run.branch}}"
  - id: after
    kind: bash
    depends_on: [approve]
    run: "false"
"#;

struct Bench {
    _root: tempfile::TempDir,
    worktree: std::path::PathBuf,
    storage: Storage,
    run_id: RunId,
    manifest: yunta_core::Manifest,
    run_dir: std::path::PathBuf,
}

impl Bench {
    async fn new() -> Self {
        Self::with_workflow(GATE_ONLY_WORKFLOW).await
    }

    async fn with_workflow(workflow_yaml: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        let run_id = RunId::from("run-gate-1");

        let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
        let manifest = build_manifest(
            &workflow,
            &ConfigLayer::default(),
            &worktree,
            &worktree,
            &HashMap::new(),
        )
        .unwrap();
        let run_dir = create_run(
            CreateRunParams {
                run_id: &run_id,
                manifest: &manifest,
                runs_root: &runs_root,
                mode: &"default".into(),
                promoted_from: None,
                artifacts: &[],
            },
            &storage.async_handle(),
            &FixedClock,
        )
        .await
        .unwrap();

        Bench {
            _root: root,
            worktree,
            storage,
            run_id,
            manifest,
            run_dir,
        }
    }

    /// One `execute_run` invocation — a fresh call each time, exactly
    /// like a separate `yunta run`/`yunta resume` process would make;
    /// nothing here carries state across calls except the log itself.
    async fn wake(
        &self,
        forge: Option<&dyn yunta_adapters::Forge>,
    ) -> (RunTerminal, yunta_engine::RunState) {
        let adapters: HashMap<AdapterId, std::sync::Arc<dyn Adapter>> = HashMap::new();
        let report = execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: &FixedClock,
            ids: &IDS,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &NoInteraction,
            forge,
            cancel: None,
            adapter_override: None,
        })
        .await
        .unwrap();
        (report.terminal, report.state)
    }
}

#[tokio::test]
async fn an_external_gate_publishes_pauses_and_resolves_on_a_separate_wake() {
    let bench = Bench::new().await;
    let forge_state = MockForgeState::new();
    let forge = MockForge::new(forge_state.clone());

    // Person A's machine: first wake reaches the gate, publishes, pauses.
    let (terminal, state) = bench.wake(Some(&forge)).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "got {terminal:?}"
    );
    // A published, unresolved gate derives `waiting` —
    // never "absent" and never running/failed.
    assert!(
        matches!(
            state.nodes.get("approve"),
            Some(NodeState::Waiting {
                external_ref: Some(_)
            })
        ),
        "a published unresolved gate must derive Waiting with its PR ref, got {:?}",
        state.nodes.get("approve")
    );
    assert!(
        forge_state.pr_number(bench.run_id.as_str()).is_some(),
        "publish must have opened a PR"
    );

    // Person B: reviews and approves directly on the forge — no Yunta
    // involved on their end at all.
    forge_state.approve(bench.run_id.as_str(), "person-b");

    // Person A's machine, a second, wholly separate `execute_run` call
    // (simulating `yunta resume`): picks the approval up on its own.
    let (terminal, state) = bench.wake(Some(&forge)).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("approve"),
        Some(NodeState::Finished { .. })
    ));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let resolved = events.iter().find_map(|e| match e.payload() {
        Some(yunta_core::events::EventPayload::GateResolved(p)) => Some(p),
        _ => None,
    });
    assert_eq!(
        resolved.and_then(|p| p.resolved_by.as_deref()),
        Some("person-b")
    );
    assert!(
        resolved.and_then(|p| p.approved_sha.as_ref()).is_some(),
        "gate_resolved must carry the approved SHA ('usuario+timestamp+SHA')"
    );
}

#[tokio::test]
async fn a_commit_after_approval_returns_the_gate_to_waiting() {
    // Needs a still-open run (see this workflow's own doc comment) —
    // `after` fails with no `on_failure`, so the run pauses rather than
    // reaching `run_finished` once the gate resolves.
    let bench = Bench::with_workflow(GATE_THEN_UNRESOLVED_WORKFLOW).await;
    let forge_state = MockForgeState::new();
    let forge = MockForge::new(forge_state.clone());

    let (terminal, _) = bench.wake(Some(&forge)).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    forge_state.approve(bench.run_id.as_str(), "person-b");
    let (terminal, state) = bench.wake(Some(&forge)).await;
    // The gate itself resolved (Finished), but `after` failed on its own
    // with nowhere to reroute — the run as a whole is still Paused, not
    // Finished, which is exactly what keeps it open to recheck.
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(matches!(
        state.nodes.get("approve"),
        Some(NodeState::Finished { .. })
    ));

    // A new commit lands on the PR *after* the approval — real forges
    // don't dismiss the old review just because the branch moved.
    forge_state.push_commit(bench.run_id.as_str());

    // A third wake must notice the approval no longer covers the
    // current head and go back to waiting.
    let (terminal, state) = bench.wake(Some(&forge)).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !matches!(state.nodes.get("approve"), Some(NodeState::Finished { .. })),
        "a stale approval must return the gate to waiting, not stay silently Finished"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let started_count = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("approve")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::NodeStarted(_))
                )
        })
        .count();
    assert_eq!(
        started_count, 2,
        "the drift recheck must re-open the node (a second node_started), not fabricate a new one"
    );

    // A fresh approval at the new head resolves it again, same as the
    // first time.
    forge_state.approve(bench.run_id.as_str(), "person-b");
    let (_, state) = bench.wake(Some(&forge)).await;
    assert!(matches!(
        state.nodes.get("approve"),
        Some(NodeState::Finished { .. })
    ));
}

#[tokio::test]
async fn changes_requested_posts_findings_and_fails_the_node_retryably() {
    let bench = Bench::new().await;
    let forge_state = MockForgeState::new();
    let forge = MockForge::new(forge_state.clone());

    bench.wake(Some(&forge)).await;
    forge_state.request_changes(
        bench.run_id.as_str(),
        "person-b",
        vec![yunta_adapters::ReviewComment {
            author: "person-b".to_string(),
            body: "please add a test".to_string(),
            path: Some("src/lib.rs".to_string()),
        }],
    );

    let (terminal, state) = bench.wake(Some(&forge)).await;
    // No `on_failure` declared on this workflow's gate, so a retryable
    // failure with nowhere to reroute to just pauses — the same rule
    // any other failed node without `on_failure` already follows.
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(matches!(
        state.nodes.get("approve"),
        Some(NodeState::Failed {
            retryable: true,
            ..
        })
    ));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let finding = events.iter().find_map(|e| match e.payload() {
        Some(yunta_core::events::EventPayload::FindingPosted(p)) => Some(&p.finding),
        _ => None,
    });
    assert_eq!(
        finding.map(|f| f.detail.as_str()),
        Some("please add a test")
    );
}

#[tokio::test]
async fn a_merged_pr_resolves_the_gate_as_approved_by_the_merger() {
    let bench = Bench::new().await;
    let forge_state = MockForgeState::new();
    let forge = MockForge::new(forge_state.clone());

    bench.wake(Some(&forge)).await;
    // Person B merges the PR outright: an approval that also landed.
    let merge_sha = forge_state.merge(bench.run_id.as_str(), "person-b");

    let (terminal, state) = bench.wake(Some(&forge)).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("approve"),
        Some(NodeState::Finished { .. })
    ));
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let resolved = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::GateResolved(p)) => Some(p),
            _ => None,
        })
        .expect("the gate resolves");
    assert_eq!(resolved.resolved_by.as_deref(), Some("person-b"));
    assert_eq!(
        resolved.approved_sha.as_deref(),
        Some(merge_sha.as_str()),
        "the evidence is the merge commit"
    );
}

#[tokio::test]
async fn a_closed_pr_fails_the_node_non_retryably() {
    let bench = Bench::new().await;
    let forge_state = MockForgeState::new();
    let forge = MockForge::new(forge_state.clone());

    bench.wake(Some(&forge)).await;
    forge_state.close(bench.run_id.as_str());

    let (_, state) = bench.wake(Some(&forge)).await;
    assert!(matches!(
        state.nodes.get("approve"),
        Some(NodeState::Failed {
            retryable: false,
            ..
        })
    ));
}

#[tokio::test]
async fn with_no_forge_the_gate_degrades_to_console_and_never_publishes() {
    let bench = Bench::new().await;
    // `NoInteraction` always reports "can't interact" — the same
    // degrade-to-pause path a headless console already takes.
    let (terminal, state) = bench.wake(None).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !state.nodes.contains_key("approve"),
        "no forge and no answer from the console must not fabricate a resolution"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateWaiting(_))
        )),
        "an unresolved degraded gate must not be recorded as published"
    );
}

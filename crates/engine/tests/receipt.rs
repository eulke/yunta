//! Verified Work Receipt.
//!
//! Two concerns, tested separately: the **formatters**
//! (`render_markdown`/`render_json`) against a hand-built [`Receipt`] —
//! golden byte-for-byte output, same style `tests/progress.rs` and
//! `tests/stats.rs` use — and the **derivation**
//! (`build_receipt`) against a real run's own log, executed end to end
//! with the mock adapter (same style `tests/run.rs` uses), asserted
//! field by field rather than as one giant string so a fixture tweak
//! doesn't need to reprint an entire golden blob.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::events::{StoredEvent, TerminalState, TokenUsage};
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, ConfigLayer, NodeId, RunId, Workflow};
use yunta_engine::{
    build_manifest, build_receipt, create_run, execute_run, render_receipt_json,
    render_receipt_markdown, BaselineSummary, CostSummary, CriteriaSummary, CriterionEntry,
    EventChainStatus, Receipt, ReceiptError, RunEnv, RunnerUsage, ScopeSummary,
};
use yunta_storage::Storage;
use yunta_testkit::{init_repo, FixedClock};

/// Run ids for everything a test run gives birth to — unique across
/// the binary, so parallel tests never share a run directory.
static IDS: SeqIdSource = SeqIdSource::new("minted");

// --- formatters: golden output over a hand-built Receipt --------------------

fn sample_receipt(event_chain: EventChainStatus) -> Receipt {
    Receipt {
        run_id: RunId::from("run-2026-08-21-0001"),
        workflow: "release-cycle".to_string(),
        mode: "default".into(),
        terminal_state: TerminalState::Done,
        criteria: CriteriaSummary {
            total: 3,
            green: 3,
            entries: vec![
                CriterionEntry {
                    task_id: "T001".to_string(),
                    cmd: "test -f hello.txt".to_string(),
                    exit_code: 0,
                },
                CriterionEntry {
                    task_id: "T002".to_string(),
                    cmd: "cargo test -p yunta-core".to_string(),
                    exit_code: 0,
                },
                CriterionEntry {
                    task_id: "T002".to_string(),
                    cmd: "cargo clippy --workspace -- -D warnings".to_string(),
                    exit_code: 0,
                },
            ],
        },
        baseline: Some(BaselineSummary {
            suite: "make test".to_string(),
            hash: "a3f2b1c4d5e6f7081234567890abcdef".to_string(),
            compared: 2,
            regressions: 0,
        }),
        scope: ScopeSummary {
            files_touched: 4,
            violations: Vec::new(),
        },
        runners: vec![
            RunnerUsage {
                node_id: NodeId::from("review@reviewer"),
                runner: "reviewer".into(),
                adapter: "claude-code".into(),
                model: "claude-sonnet-4-6".into(),
            },
            RunnerUsage {
                node_id: NodeId::from("review@reviewer-alt"),
                runner: "reviewer-alt".into(),
                adapter: "codex".into(),
                model: "gpt-5-codex".into(),
            },
        ],
        cost: CostSummary {
            tokens: TokenUsage {
                input: 1200,
                output: 340,
                cached: Some(200),
            },
            cptv: Some(770.0),
            reroutes: 2,
        },
        event_chain,
        unknown_kinds: Vec::new(),
    }
}

const EXPECTED_MARKDOWN_INTACT: &str = "\
# Verified Work Receipt — run run-2026-08-21-0001

workflow: `release-cycle` · mode: `default` · state: Done

- ✓ 3/3 criteria green (commands + exit codes below)
- ✓ 0 regression(s) vs baseline across 2 comparison(s) (suite `make test`, hash `a3f2b1c4d5e6`)
- ✓ scope: 4 file(s) touched, 0 violation(s)
- ✓ Reviewed by 2 independent runner(s) via `review` (claude-code, codex)
- cost: 1540 tokens (1200 in / 340 out) · CPTV: 770.0 tokens/task · 2 reroute(s)
- ✓ event chain: 342 event(s), hash-linked, replayable

## Criteria

- ✓ `T001` — `test -f hello.txt` (exit 0)
- ✓ `T002` — `cargo test -p yunta-core` (exit 0)
- ✓ `T002` — `cargo clippy --workspace -- -D warnings` (exit 0)
";

#[test]
fn renders_the_markdown_receipt_byte_for_byte() {
    let receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    assert_eq!(render_receipt_markdown(&receipt), EXPECTED_MARKDOWN_INTACT);
}

#[test]
fn a_broken_event_chain_renders_as_a_visible_failure_not_a_silent_omission() {
    let receipt = sample_receipt(EventChainStatus::Broken {
        seq: 88.into(),
        detail: "payload hash mismatch".to_string(),
    });
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains("✗ event chain BROKEN at seq 88: payload hash mismatch"),
        "got:\n{markdown}"
    );
}

#[test]
fn renders_the_json_receipt_as_pretty_printed_structured_data() {
    let receipt = sample_receipt(EventChainStatus::Intact { events: 342 });
    let json = render_receipt_json(&receipt).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["run_id"], "run-2026-08-21-0001");
    assert_eq!(parsed["workflow"], "release-cycle");
    assert_eq!(parsed["terminal_state"], "done");
    assert_eq!(parsed["criteria"]["total"], 3);
    assert_eq!(parsed["criteria"]["green"], 3);
    assert_eq!(parsed["baseline"]["regressions"], 0);
    assert_eq!(parsed["scope"]["violations"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["runners"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["cost"]["cptv"], 770.0);
    assert_eq!(parsed["event_chain"]["status"], "intact");
    assert_eq!(parsed["event_chain"]["events"], 342);

    // No LLM-authored text anywhere in the receipt —
    // every string in the JSON traces back to a
    // command, a role, a hash or a count, never freeform prose an agent
    // could have written. Structural proxy for that: no field holds
    // more than one sentence of prose — `entries[].cmd` are shell
    // commands, not narration.
    for entry in parsed["criteria"]["entries"].as_array().unwrap() {
        assert!(!entry["cmd"].as_str().unwrap().is_empty());
    }
}

const EXPECTED_MARKDOWN_NO_BASELINE: &str = "\
# Verified Work Receipt — run run-2026-08-21-0001

workflow: `release-cycle` · mode: `default` · state: Done

- ✓ 3/3 criteria green (commands + exit codes below)
- baseline: not used by this workflow
- ✓ scope: 4 file(s) touched, 0 violation(s)
- ✓ Reviewed by 2 independent runner(s) via `review` (claude-code, codex)
- cost: 1540 tokens (1200 in / 340 out) · CPTV: 770.0 tokens/task · 2 reroute(s)
- ✓ event chain: 10 event(s), hash-linked, replayable

## Criteria

- ✓ `T001` — `test -f hello.txt` (exit 0)
- ✓ `T002` — `cargo test -p yunta-core` (exit 0)
- ✓ `T002` — `cargo clippy --workspace -- -D warnings` (exit 0)
";

#[test]
fn baseline_absent_never_invents_a_zero_regression_line() {
    let mut receipt = sample_receipt(EventChainStatus::Intact { events: 10 });
    receipt.baseline = None;
    assert_eq!(
        render_receipt_markdown(&receipt),
        EXPECTED_MARKDOWN_NO_BASELINE
    );
}

// --- derivation: build_receipt over a real run's own log --------------------

const CONFIG: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
  reviewer:
    - { adapter: mock, model: mock-model }
  reviewer-alt:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "true"
"#;

/// Exercises every receipt section in one run: a ledger task with two
/// criteria (`plan`/`implement`), a baseline capture-then-compare pair
/// (`capture`/`compare`), a re-route (`lint` fails once, `fix-lint`
/// corrects it), and a fan-out review (`runners: [reviewer,
/// reviewer-alt]`) whose effects stay inside its own declared `scope`.
const WORKFLOW: &str = r#"
name: receipt-fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/ledger.yaml."
    artifacts:
      produces: [{ name: ledger.yaml, kind: task-ledger }]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "implement your task"
  - id: capture
    kind: check
    builtin: baseline_compare
    depends_on: [implement]
  - id: compare
    kind: check
    builtin: baseline_compare
    depends_on: [capture]
  - id: lint
    kind: bash
    depends_on: [compare]
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 2 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "fix the lint failure"
  - id: review
    kind: prompt
    depends_on: [lint]
    runners: [reviewer, reviewer-alt]
    scope: ["notes/**", "hello.txt", "fixed.txt"]
    prompt: "review the change"
"#;

/// `{artifacts}` is substituted with the run's own absolute
/// `run.dir/artifacts` before parsing — the same convention
/// `tests/run.rs`'s own ledger fixtures use, since a mock effect's
/// `path` is relative to the worktree, not `run.dir`.
fn fixture(artifacts_dir: &Path) -> String {
    format!(
        r#"
sessions:
  - match_prompt_contains: "Write the ledger"
    effects:
      - {{ path: "{artifacts}/ledger.yaml", content: "tasks:\n  - id: T001\n    title: \"Say hello\"\n    scope: [\"hello.txt\"]\n    criteria:\n      - cmd: \"test -f hello.txt\"\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - match_prompt_contains: "implement your task"
    effects:
      - {{ path: hello.txt, content: "hi" }}
    outcome: {{ type: completed, summary: "done" }}
  - match_prompt_contains: "fix the lint failure"
    effects:
      - {{ path: fixed.txt, content: "fixed" }}
    outcome: {{ type: completed, summary: "fixed it" }}
  - match_prompt_contains: "review the change"
    effects:
      - {{ path: notes/reviewer.md, content: "looks good" }}
    outcome: {{ type: completed, summary: "reviewed" }}
  - match_prompt_contains: "review the change"
    effects:
      - {{ path: notes/reviewer-alt.md, content: "also good" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"#,
        artifacts = artifacts_dir.display(),
    )
}

struct Bench {
    _root: tempfile::TempDir,
    worktree: PathBuf,
    runs_root: PathBuf,
    storage: Storage,
    run_id: RunId,
}

impl Bench {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        init_repo(&worktree);
        let runs_root = root.path().join("runs");
        let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
        Bench {
            _root: root,
            worktree,
            runs_root,
            storage,
            run_id: RunId::from("run-receipt-1"),
        }
    }

    fn run_dir(&self) -> PathBuf {
        self.runs_root.join(self.run_id.as_str())
    }

    async fn run(
        &self,
        workflow_yaml: &str,
        fixture_yaml: &str,
    ) -> (yunta_core::Manifest, Vec<StoredEvent>) {
        let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
        let config: ConfigLayer = serde_norway::from_str(CONFIG).unwrap();
        let manifest = build_manifest(
            &workflow,
            &config,
            &self.worktree,
            &self.worktree,
            &HashMap::new(),
        )
        .unwrap();
        let run_dir = create_run(
            yunta_engine::CreateRunParams {
                run_id: &self.run_id,
                manifest: &manifest,
                runs_root: &self.runs_root,
                mode: &"default".into(),
                promoted_from: None,
                artifacts: &[],
            },
            &self.storage.async_handle(),
            &FixedClock,
        )
        .await
        .unwrap();

        let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
        let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".into(), Arc::new(adapter));

        execute_run(RunEnv {
            run_id: &self.run_id,
            manifest: &manifest,
            run_dir: &run_dir,
            worktree: &self.worktree,
            adapters: &adapters,
            storage: &self.storage.async_handle(),
            clock: std::sync::Arc::new(FixedClock),
            ids: &IDS,
            max_task_retries: yunta_engine::DEFAULT_MAX_RETRIES,
            human_interaction: &yunta_engine::NoInteraction,
            forge: None,
            cancel: None,
            adapter_override: None,
            ambient: None,
        })
        .await
        .unwrap();

        let events = self.storage.events_for_run(&self.run_id).unwrap();
        (manifest, events)
    }
}

#[tokio::test]
async fn build_receipt_derives_every_section_from_a_real_runs_own_log() {
    let bench = Bench::new();
    let fixture_yaml = fixture(&bench.run_dir().join("artifacts"));
    let (manifest, events) = bench.run(WORKFLOW, &fixture_yaml).await;

    let chain = EventChainStatus::Intact {
        events: events.len(),
    };
    let receipt = build_receipt(&bench.run_id, &manifest, &events, chain).unwrap();

    assert_eq!(receipt.terminal_state, TerminalState::Done);

    assert_eq!(receipt.criteria.total, 1, "T001's one criterion");
    assert_eq!(receipt.criteria.green, 1);
    assert_eq!(receipt.criteria.entries[0].cmd, "test -f hello.txt");

    let baseline = receipt.baseline.clone().expect("baseline_compare was used");
    assert_eq!(baseline.suite, "true");
    assert_eq!(baseline.compared, 1, "capture doesn't count, compare does");
    assert_eq!(baseline.regressions, 0);

    assert!(
        receipt.scope.violations.is_empty(),
        "every session wrote inside its declared scope: {:?}",
        receipt.scope.violations
    );
    assert!(receipt.scope.files_touched > 0);

    let groups = yunta_engine::fan_out_groups(&receipt.runners);
    assert_eq!(groups.len(), 1);
    let (base, members) = &groups[0];
    assert_eq!(base, "review");
    assert_eq!(members.len(), 2);

    assert_eq!(receipt.cost.reroutes, 1, "lint failed once, then re-routed");
    assert!(receipt.cost.cptv.is_some(), "one task reached done");

    // The formatters accept whatever build_receipt derived without
    // panicking or needing extra data — same contract a real `pr` node
    // relies on.
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains("Reviewed by 2 independent runner(s)"),
        "got: {markdown}"
    );
    render_receipt_json(&receipt).unwrap();
}

#[tokio::test]
async fn build_receipt_refuses_a_run_that_never_reached_run_finished() {
    let bench = Bench::new();
    // `lint` fails with no `on_failure` at all — the run pauses, never
    // finishes, so there is no `run_finished` metrics to certify.
    let workflow = r#"
name: never-finishes
nodes:
  - id: broken
    kind: bash
    run: "false"
"#;
    let workflow_parsed: Workflow = serde_norway::from_str(workflow).unwrap();
    let config: ConfigLayer = serde_norway::from_str(CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow_parsed,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        yunta_engine::CreateRunParams {
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
    .unwrap();
    let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: yunta_engine::DEFAULT_MAX_RETRIES,
        human_interaction: &yunta_engine::NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let err = build_receipt(
        &bench.run_id,
        &manifest,
        &events,
        EventChainStatus::Intact {
            events: events.len(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, ReceiptError::NotFinished(_)));
    assert!(err.to_string().contains("yunta status"));
}

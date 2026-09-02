//! `yunta/fragua` runs its full reference pipeline end to end with the
//! `mock` adapter, `pr` included. The pack's own `.yunta/tests/` cases
//! stop at the first gate: a case has no human to answer a gate and no
//! `gh` to call. This test drives `mode: quick` directly through the
//! engine, approving `ship` the way an operator's own automation would
//! and stubbing `gh` so the `pr` node's real command has something to
//! call.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, HumanInteraction, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }
}

/// Always picks the escalation's first option — for `ship`'s
/// `[approve]` (the default single option), that's approval; the
/// same stand-in `workflow_compose.rs`'s own release-cycle test uses.
struct ApproveEverything;

#[async_trait::async_trait]
impl HumanInteraction for ApproveEverything {
    async fn resolve(
        &self,
        escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        Some(yunta_core::events::GateResolvedPayload {
            chosen_option: escalation.options.first().map(|o| o.id.clone()),
            resolved_by: Some("test".to_string()),
            free_text: None,
            approved_sha: None,
        })
    }
}

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

const CONFIG: &str = r#"
project:
  base_branch: master
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "cargo test"
"#;

#[tokio::test]
async fn yunta_fragua_build_feature_runs_end_to_end_in_quick_mode_with_mock() {
    // `gh` isn't installed in this environment (or anywhere CI runs) —
    // stub it so the `pr` node's real bash command has something to
    // call. This is the only test in this file, so mutating PATH for
    // the process is safe: nothing else in this binary runs concurrently
    // against a `gh` invocation.
    let stub_dir = tempfile::tempdir().unwrap();
    let gh_stub = stub_dir.path().join("gh");
    write(&gh_stub, "#!/bin/sh\necho \"pr created (stub): $*\"\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh_stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{path}", stub_dir.path().display()));

    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    write(
        &worktree.join("Cargo.toml"),
        "[package]\nname = \"sandbox\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &worktree.join("src/lib.rs"),
        "pub fn hello() -> &'static str {\n    \"hello\"\n}\n",
    );
    write(
        &worktree.join("docs/architecture.md"),
        "# Architecture\n\nA sandbox crate for the fragua reference pipeline's own test.\n",
    );
    git(&worktree, &["init", "-q", "-b", "master"]);
    git(&worktree, &["config", "user.email", "test@example.com"]);
    git(&worktree, &["config", "user.name", "Test"]);
    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-q", "-m", "initial"]);

    // `pr`'s own `git push -u origin {{run.branch}}` needs a real
    // remote and a local branch of exactly that name — both of which a
    // real `isolation: worktree` run gets from `prepare_worktree`
    // before any node executes. This test calls `execute_run` directly,
    // so it recreates that same setup by hand instead of going through
    // `prepare_worktree`.
    let bare = root.path().join("origin.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare"]);
    git(
        &worktree,
        &["remote", "add", "origin", bare.to_str().unwrap()],
    );
    let run_id = RunId::from("run-fragua");
    git(
        &worktree,
        &["checkout", "-q", "-b", &format!("yunta/{run_id}")],
    );

    let workflow_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packs/fragua/.yunta/workflows/build-feature.yaml"
    );
    let workflow_yaml = std::fs::read_to_string(workflow_path).unwrap();
    let workflow: Workflow = serde_yaml::from_str(&workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let workflow_dir = Path::new(workflow_path).parent().unwrap();
    let inputs = HashMap::from([("idea".to_string(), "add dark mode".to_string())]);
    let manifest = build_manifest(&workflow, &config, workflow_dir, &worktree, &inputs).unwrap();

    let runs_root = root.path().join("runs");
    let run_dir = runs_root.join(run_id.as_str());
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: "quick",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();

    // One scripted session per node the "quick" mode actually spawns,
    // in the order the DAG reaches them: grill, plan, one implement
    // task, then lint/tests/ship/pr run for real against the sandbox
    // crate above (no mock involved — cargo and git are the real
    // things being exercised, exactly as they would be in production).
    // A session's own cwd is the worktree, not run.dir — artifacts
    // (unlike the loop's own scope-relative edits below) need the
    // absolute run.dir path, the same one a real agent would be given
    // in its rendered prompt.
    let artifacts = run_dir.join("artifacts");
    let fixture = format!(
        r##"
sessions:
  - effects:
      - {{ path: {questions:?}, content: "questions: []\n" }}
      - {{ path: {brief:?}, content: "# Brief\n\nAdd dark mode.\n" }}
    outcome: {{ type: completed, summary: "grilled" }}
  - effects:
      - path: {plan:?}
        content: |
          tasks:
            - id: T001
              title: "Document the sandbox crate"
              scope: ["src/lib.rs"]
              criteria:
                - cmd: "grep -q '//! sandbox' src/lib.rs"
    outcome: {{ type: completed, summary: "planned" }}
  - effects:
      - path: "src/lib.rs"
        content: |
          //! sandbox

          pub fn hello() -> &'static str {{
              "hello"
          }}
    outcome: {{ type: completed, summary: "did T001" }}
"##,
        questions = artifacts.join("questions.yaml"),
        brief = artifacts.join("brief.md"),
        plan = artifacts.join("plan.yaml"),
    );
    let adapter = MockAdapter::from_yaml(&fixture).unwrap();
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), Arc::new(adapter));

    let report = execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage,
        clock: &FixedClock,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &ApproveEverything,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    assert_eq!(
        report.terminal,
        RunTerminal::Finished,
        "state: {:?}",
        report.state
    );
    for node in ["grill", "plan", "implement", "lint", "tests", "ship", "pr"] {
        assert!(
            matches!(
                report.state.nodes.get(&node.into()),
                Some(yunta_engine::NodeState::Finished { .. })
            ),
            "node `{node}` did not finish: {:?}",
            report.state.nodes.get(&node.into())
        );
    }
    // `fix-lint` is only in quick mode's node set as a re-route target —
    // lint passed on the first try, so it must never have run.
    assert!(
        !report.state.nodes.contains_key(&"fix-lint".into()),
        "fix-lint ran despite lint passing on the first try: {:?}",
        report.state.nodes.get(&"fix-lint".into())
    );
}

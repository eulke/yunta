//! `promote-knowledge` (T10.5, D56/RFC-0003 §4): the reference workflow
//! for curating locally-distilled knowledge into a new version of the
//! org knowledge pack — "no es automática y no la hace el engine, es
//! un workflow de Yunta como cualquier otro." Runs end to end with
//! mock, and never publishes without an actually-approved gate.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{Clock, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, check, create_run, execute_run, CreateRunParams, HumanInteraction,
    NoInteraction, NodeState, RunTerminal, DEFAULT_MAX_RETRIES,
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

/// Always picks the escalation's first declared option — `approve`,
/// here (the engine appends its own `abort` after whatever the
/// workflow declares, §5.3).
struct ApproveEverything;

#[async_trait::async_trait]
impl HumanInteraction for ApproveEverything {
    async fn resolve(
        &self,
        escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::GateResolvedPayload> {
        Some(yunta_core::events::GateResolvedPayload {
            chosen_option: escalation.options.first().map(|o| o.id.clone()),
            resolved_by: Some("curator".to_string()),
            free_text: None,
            approved_sha: None,
        })
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

const CONFIG: &str = r#"
runners:
  curator:
    - { adapter: mock, model: mock-model }
"#;

/// An org-knowledge pack's own repo: `pack.yaml` (the thing `publish`
/// version-bumps and tags), an existing `knowledge/` entry, and a
/// curator-gathered `candidates.md` naming what's up for promotion.
fn org_pack_worktree(root: &Path) -> std::path::PathBuf {
    let worktree = root.join("worktree");
    write(
        &worktree.join("pack.yaml"),
        "name: org-knowledge\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
         permissions: read-only\ncontents:\n  knowledge: [knowledge/]\n",
    );
    write(
        &worktree.join("knowledge/existing.md"),
        "# Existing convention\n\nAlready promoted.\n",
    );
    write(
        &worktree.join("candidates.md"),
        "# Candidate: retry budgets\n\nFrom repo acme/api, distilled 2026-01-01.\n",
    );
    git(&worktree, &["init", "-q", "-b", "master"]);
    git(&worktree, &["config", "user.email", "test@example.com"]);
    git(&worktree, &["config", "user.name", "Test"]);
    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-q", "-m", "initial"]);
    worktree
}

fn build(worktree: &Path, inputs: &HashMap<String, String>) -> yunta_core::Manifest {
    let workflow_yaml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/promote-knowledge.yaml"
    ))
    .unwrap();
    let workflow: Workflow = serde_yaml::from_str(&workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    build_manifest(&workflow, &config, worktree, worktree, inputs).unwrap()
}

#[test]
fn the_reference_workflow_passes_static_check() {
    let workflow_yaml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/promote-knowledge.yaml"
    ))
    .unwrap();
    let workflow: Workflow = serde_yaml::from_str(&workflow_yaml).unwrap();
    let config: ConfigLayer = serde_yaml::from_str(CONFIG).unwrap();
    let errors = check(&workflow, &config);
    assert!(errors.is_empty(), "{errors:?}");
}

#[tokio::test]
async fn an_approved_gate_publishes_the_new_version() {
    let root = tempfile::tempdir().unwrap();
    let worktree = org_pack_worktree(root.path());
    let inputs = HashMap::from([
        ("candidates".to_string(), "candidates.md".to_string()),
        ("new_version".to_string(), "1.1.0".to_string()),
    ]);
    let manifest = build(&worktree, &inputs);

    let run_id = RunId::from("run-promote");
    let runs_root = root.path().join("runs");
    let run_dir = runs_root.join(run_id.as_str());
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();

    let notes_path = run_dir.join("artifacts/promotion-notes.md");
    let fixture = format!(
        r##"
sessions:
  - effects:
      - path: "knowledge/retry-budgets.md"
        content: |
          # Retry budgets

          From repo acme/api, distilled 2026-01-01.
      - {{ path: {notes:?}, content: "promoted retry-budgets; nothing left out\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"##,
        notes = notes_path,
    );
    let adapter = MockAdapter::from_yaml(&fixture).unwrap();
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), Arc::new(adapter));

    let report = execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &ApproveEverything,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        report.terminal,
        RunTerminal::Finished,
        "state: {:?}",
        report.state
    );
    assert!(matches!(
        report.state.nodes.get(&"publish".into()),
        Some(NodeState::Finished { .. })
    ));

    let pack_yaml = std::fs::read_to_string(worktree.join("pack.yaml")).unwrap();
    assert!(pack_yaml.contains("version: 1.1.0"), "{pack_yaml}");
    let tags = git(&worktree, &["tag", "--list"]);
    assert_eq!(tags, "v1.1.0");
    let log = git(&worktree, &["log", "--oneline", "-1"]);
    assert!(log.contains("promote to 1.1.0"), "{log}");
}

#[tokio::test]
async fn an_unresolved_gate_never_publishes_anything() {
    let root = tempfile::tempdir().unwrap();
    let worktree = org_pack_worktree(root.path());
    let inputs = HashMap::from([
        ("candidates".to_string(), "candidates.md".to_string()),
        ("new_version".to_string(), "1.1.0".to_string()),
    ]);
    let manifest = build(&worktree, &inputs);

    let run_id = RunId::from("run-promote-unresolved");
    let runs_root = root.path().join("runs");
    let run_dir = runs_root.join(run_id.as_str());
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: "default",
            promoted_from: None,
        },
        &storage,
        &FixedClock,
    )
    .unwrap();

    let notes_path = run_dir.join("artifacts/promotion-notes.md");
    let fixture = format!(
        r##"
sessions:
  - effects:
      - {{ path: {notes:?}, content: "promoted retry-budgets; nothing left out\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
"##,
        notes = notes_path,
    );
    let adapter = MockAdapter::from_yaml(&fixture).unwrap();
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".to_string(), Arc::new(adapter));

    // No surface to ask — the same conservative default `yunta test`
    // and headless CI use: a gate no one can answer must pause, never
    // guess, and everything behind it (here: the entire publish step)
    // must never run.
    let report = execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &FixedClock,
        DEFAULT_MAX_RETRIES,
        &NoInteraction,
        None,
        None,
    )
    .await
    .unwrap();

    assert!(
        matches!(report.terminal, RunTerminal::Paused { .. }),
        "terminal: {:?}",
        report.terminal
    );
    assert!(
        !report.state.nodes.contains_key(&"publish".into()),
        "publish ran despite the gate never being resolved: {:?}",
        report.state.nodes.get(&"publish".into())
    );

    let pack_yaml = std::fs::read_to_string(worktree.join("pack.yaml")).unwrap();
    assert!(
        pack_yaml.contains("version: 1.0.0"),
        "version must not have moved: {pack_yaml}"
    );
    let tags = git(&worktree, &["tag", "--list"]);
    assert!(tags.is_empty(), "no tag should exist yet: {tags:?}");
}

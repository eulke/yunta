//! `promote-knowledge`: the reference workflow
//! for curating locally-distilled knowledge into a new version of the
//! org knowledge pack — "no es automática y no la hace el engine, es
//! un workflow de Yunta como cualquier otro." Runs end to end with
//! mock, and never publishes without an actually-approved gate.

use std::path::Path;

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{check, NodeState, RunReport, RunTerminal};
use yunta_testkit::{git, git_output, write, ApproveEverything, Bench};

/// The reference workflow as the repo ships it, checked and run from the
/// one copy every other reader of it also gets.
const WORKFLOW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../core/tests/fixtures/promote-knowledge.yaml"
));

const CONFIG: &str = r#"
runners:
  curator:
    - { adapter: mock, model: mock-model }
"#;

/// A curator who copies one candidate into `knowledge/` verbatim and
/// records what it promoted in the artifact the node declares.
const PROMOTION_FIXTURE: &str = r#"
sessions:
  - effects:
      - path: "knowledge/retry-budgets.md"
        content: |
          # Retry budgets

          From repo acme/api, distilled 2026-01-01.
      - path: "{{run.staging}}/review-candidates/promotion-notes.md"
        content: "promoted retry-budgets; nothing left out\n"
    outcome: { type: completed, summary: "reviewed" }
"#;

/// The same review, writing only its notes: what the worktree holds when
/// nothing behind the gate ever runs.
const REVIEW_ONLY_FIXTURE: &str = r#"
sessions:
  - effects:
      - path: "{{run.staging}}/review-candidates/promotion-notes.md"
        content: "promoted retry-budgets; nothing left out\n"
    outcome: { type: completed, summary: "reviewed" }
"#;

/// Lays an org-knowledge pack's own repo over `worktree`: `pack.yaml`
/// (the thing `publish` version-bumps and tags), an existing
/// `knowledge/` entry, and a curator-gathered `candidates.md` naming
/// what's up for promotion.
fn org_pack(worktree: &Path) {
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
    git(worktree, &["add", "."]);
    git(worktree, &["commit", "-q", "-m", "pack"]);
}

/// A bench whose worktree is the org pack's repo, invoked the way a
/// curator invokes the workflow.
fn curator_bench(run_id: &str) -> Bench {
    let bench = Bench::with_run_id(run_id)
        .with_inputs(&[("candidates", "candidates.md"), ("new_version", "1.1.0")]);
    org_pack(&bench.worktree);
    bench
}

#[test]
fn the_reference_workflow_passes_static_check() {
    let workflow: Workflow = serde_norway::from_str(WORKFLOW).unwrap();
    let config: ConfigLayer = serde_norway::from_str(CONFIG).unwrap();
    let errors = check(&workflow, &config, &|_| None);
    assert!(errors.is_empty(), "{errors:?}");
}

#[tokio::test]
async fn an_approved_gate_publishes_the_new_version() {
    let bench = curator_bench("run-promote");

    let RunReport { terminal, state } = bench
        .run_full(
            WORKFLOW,
            PROMOTION_FIXTURE,
            CONFIG,
            &ApproveEverything::new("curator"),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
    assert!(matches!(
        state.nodes.state("publish"),
        Some(NodeState::Finished { .. })
    ));

    let pack_yaml = std::fs::read_to_string(bench.worktree.join("pack.yaml")).unwrap();
    let pack: serde_norway::Value = serde_norway::from_str(&pack_yaml).unwrap();
    assert_eq!(pack["version"].as_str(), Some("1.1.0"));
    let tags = git_output(&bench.worktree, &["tag", "--list"]);
    assert_eq!(tags, "v1.1.0");
    let subject = git_output(&bench.worktree, &["log", "-1", "--format=%s"]);
    assert_eq!(subject, "knowledge: promote to 1.1.0");
}

#[tokio::test]
async fn an_unresolved_gate_never_publishes_anything() {
    let bench = curator_bench("run-promote-unresolved");

    // No surface to ask — the same conservative default `yunta test`
    // and headless CI use: a gate no one can answer must pause, never
    // guess, and everything behind it (here: the entire publish step)
    // must never run.
    let RunReport { terminal, state } = bench
        .run_with_config(WORKFLOW, REVIEW_ONLY_FIXTURE, CONFIG)
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "terminal: {terminal:?}"
    );
    assert!(
        !state.nodes.has_state("publish"),
        "publish ran despite the gate never being resolved: {:?}",
        state.nodes.state("publish")
    );

    let pack_yaml = std::fs::read_to_string(bench.worktree.join("pack.yaml")).unwrap();
    assert!(
        pack_yaml.contains("version: 1.0.0"),
        "version must not have moved: {pack_yaml}"
    );
    let tags = git_output(&bench.worktree, &["tag", "--list"]);
    assert!(tags.is_empty(), "no tag should exist yet: {tags:?}");
}

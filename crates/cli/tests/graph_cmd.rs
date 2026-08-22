//! `yunta graph`: a pure derivation of the DAG from the workflow definition.
//!
//! The schema only has `prompt`/`bash`/`loop` nodes with `depends_on`
//! and `on_failure.goto` — no `parallel`/`gate`/`workflow` composition
//! (those come later), so this covers exactly
//! that: a Mermaid `graph TD` with `depends_on` edges, `on_failure.goto`
//! edges visually differentiated from them, and — given a `--run <id>` —
//! the same graph with each node's derived state (via
//! `yunta_engine::derive`) annotated, no new events, no agent involved in
//! producing the graph itself (pure derivation, same as `status`).

use std::path::Path;
use std::process::{Command, Output};

fn yunta_in(dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .output()
        .expect("failed to run the yunta binary")
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
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

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const WORKFLOW: &str = r#"
name: graph-fixture
nodes:
  - id: lint
    kind: bash
    run: "true"
    on_failure: { goto: fix-lint, max_reroutes: 2 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: tests
    kind: bash
    depends_on: [lint]
    run: "true"
"#;

#[test]
fn graph_renders_mermaid_with_depends_on_and_differentiated_reroute_edges() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(&repo.join("wf.yaml"), WORKFLOW);

    let output = yunta_in(&repo, &home, &["graph", "wf.yaml"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);

    assert!(text.trim_start().starts_with("graph TD"), "got: {text}");
    for id in ["lint", "fix-lint", "tests"] {
        assert!(text.contains(id), "missing node `{id}` in: {text}");
    }
    // depends_on: tests depends on lint -> lint runs before tests.
    assert!(
        text.contains("lint --> tests") || text.contains("lint-->tests"),
        "missing depends_on edge lint->tests in: {text}"
    );
    // on_failure.goto must be visually different from a plain depends_on
    // edge (dashed `-.->`, a differentiated re-route edge) —
    // and never rendered as a plain `-->`.
    assert!(
        text.contains("lint -.-> fix-lint") || text.contains("lint-.->fix-lint"),
        "missing differentiated re-route edge lint->fix-lint in: {text}"
    );
    assert!(
        !text.contains("lint --> fix-lint"),
        "re-route edge must not look like a plain dependency edge: {text}"
    );
}

#[test]
fn graph_with_a_run_id_annotates_each_node_with_its_derived_state() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: graph-run-fixture
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "run failed: {}", stdout(&run));
    let run_id = stdout(&run)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .expect("run id in output");

    let output = yunta_in(&repo, &home, &["graph", "wf.yaml", "--run", &run_id]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);
    assert!(
        text.to_lowercase().contains("finished"),
        "expected the finished node's derived state in: {text}"
    );
}

#[test]
fn graph_refuses_a_workflow_that_fails_check() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // depends_on references a node that doesn't exist — check must catch
    // this before graph tries to render anything.
    write(
        &repo.join("wf.yaml"),
        r#"
name: broken
nodes:
  - id: only
    kind: bash
    run: "true"
    depends_on: [ghost]
"#,
    );

    let output = yunta_in(&repo, &home, &["graph", "wf.yaml"]);
    assert!(!output.status.success());
}

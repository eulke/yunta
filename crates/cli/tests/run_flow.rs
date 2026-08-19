//! End-to-end CLI flows: `yunta run` on a bash-only workflow (no agent
//! adapter needed), `status` over its log, `resume` idempotence,
//! `yunta test` driving a workflow with the mock adapter from a case
//! file (the designed home for mock fixtures, §14), and `yunta run`
//! actually spawning the real `claude-code` adapter (T7.3) against a
//! scripted fake `claude` binary — no network, no cost (A8).

use std::path::{Path, PathBuf};
use std::process::Output;

fn claude_code_stub() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../adapters/tests/fixtures/claude_code_stub.sh")
}

fn yunta_in(dir: &Path, home: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .output()
        .expect("failed to run the yunta binary")
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

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_id_from(output: &Output) -> String {
    stdout(output)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .expect("run id in output")
}

#[test]
fn a_bash_only_workflow_runs_from_the_cli_and_status_reads_it_back() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: bash-only
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
  - id: verify
    kind: bash
    depends_on: [touch]
    run: "test -f made.txt"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"));

    let run_id = run_id_from(&run);
    let status = yunta_in(&repo, &home, &["status", &run_id]);
    assert!(status.status.success());
    let text = stdout(&status);
    assert!(text.contains("finished"), "got: {text}");
    assert!(text.contains("touch"), "got: {text}");
    assert!(text.contains("verify"), "got: {text}");

    // Resuming a finished run is a clean no-op.
    let resume = yunta_in(&repo, &home, &["resume", &run_id]);
    assert!(
        resume.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resume.stderr)
    );
    assert!(stdout(&resume).contains("finished"));
}

#[test]
fn a_workflow_needing_agents_is_refused_before_creating_any_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // `codex` names a real adapter in the schema but has no built
    // implementation (out of M-0 scope) — exactly the case this refusal
    // exists for. `claude-code` itself is built (T7.3), so it can no
    // longer stand in for "an adapter this binary can't run" here.
    write(
        &repo.join(".yunta/config.yaml"),
        r#"
runners:
  executor:
    - { adapter: codex, model: some-model }
"#,
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: needs-agent
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("yunta test"), "got: {stderr}");
    assert!(
        !home.join("runs").exists(),
        "no run must be created when the refusal happens up front"
    );
}

#[test]
fn yunta_test_runs_a_case_with_the_mock_adapter_and_checks_expectations() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        r#"
runners:
  planner:
    - { adapter: claude-code, model: real-model }
  executor:
    - { adapter: claude-code, model: real-model }
"#,
    );
    write(
        &repo.join(".yunta/workflows/mini-bootstrap.yaml"),
        r#"
name: mini-bootstrap
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#,
    );
    // The fixture is rendered with {{run.dir}} before parsing — the
    // scripted planner writes its artifact where a real agent would.
    write(
        &repo.join(".yunta/tests/fixtures/happy.yaml"),
        r#"
sessions:
  - effects:
      - path: "{{run.dir}}/artifacts/plan.yaml"
        content: "tasks:\n  - id: T001\n    title: \"Make it\"\n    scope: [\"made.txt\"]\n    criteria:\n      - cmd: \"test -f made.txt\"\n"
    outcome: { type: completed, summary: "planned" }
  - effects:
      - { path: made.txt, content: "made" }
    outcome: { type: completed, summary: "made it" }
"#,
    );
    write(
        &repo.join(".yunta/tests/happy-path.yaml"),
        r#"
workflow: mini-bootstrap
fixture: fixtures/happy.yaml
expect:
  final_state: finished
  nodes:
    plan: finished
    implement: finished
  tasks:
    T001: done
"#,
    );

    let output = yunta_in(&repo, &home, &["test"]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "stdout: {text}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("case happy-path ... ok"), "got: {text}");
}

#[test]
fn a_failing_expectation_fails_yunta_test_naming_the_mismatch() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/one-bash.yaml"),
        r#"
name: one-bash
nodes:
  - id: fails
    kind: bash
    run: "exit 1"
"#,
    );
    write(
        &repo.join(".yunta/tests/fixtures/empty.yaml"),
        "sessions: []\n",
    );
    write(
        &repo.join(".yunta/tests/wrong-expect.yaml"),
        r#"
workflow: one-bash
fixture: fixtures/empty.yaml
expect:
  final_state: finished
"#,
    );

    let output = yunta_in(&repo, &home, &["test"]);
    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(text.contains("FAILED"), "got: {text}");
    assert!(text.contains("final_state"), "got: {text}");
}

#[test]
fn yunta_run_actually_spawns_the_real_claude_code_adapter() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            r#"
runners:
  executor:
    - {{ adapter: claude-code, model: some-model }}
adapters:
  claude-code:
    binary: {binary}
"#,
            binary = claude_code_stub().display()
        ),
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: real-adapter-smoke
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );
    // Read by the stub's fallback path (relative to the worktree it runs
    // in, since a real run's SessionRequest.env carries only secrets).
    write(
        &repo.join(".claude-stub-lines.jsonl"),
        &format!(
            "{}\n{}\n",
            r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}"#,
            r#"{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":3,"output_tokens":2}}"#,
        ),
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"), "got: {}", stdout(&run));
}

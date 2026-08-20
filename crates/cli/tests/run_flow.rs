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

    // `adapter:` isn't a closed enum in the schema — any name `runners:`
    // declares that `real_adapters` doesn't recognize is exactly "an
    // adapter this binary can't run." Both real adapters this binary
    // does build (`claude-code`, T7.3; `codex`, T7.4) are ruled out on
    // purpose, so this can't accidentally start passing once a third
    // one lands.
    write(
        &repo.join(".yunta/config.yaml"),
        r#"
runners:
  executor:
    - { adapter: some-future-cli, model: some-model }
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
    // Isolation `worktree` checks out the run's own dedicated worktree
    // from the base commit, so the fixture must be committed to reach it.
    write(
        &repo.join(".claude-stub-lines.jsonl"),
        &format!(
            "{}\n{}\n",
            r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}"#,
            r#"{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":3,"output_tokens":2}}"#,
        ),
    );
    git(&repo, &["add", ".claude-stub-lines.jsonl"]);
    git(&repo, &["commit", "-q", "-m", "stub fixture"]);

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"), "got: {}", stdout(&run));
}

#[test]
fn worktree_isolation_is_the_default_and_agent_edits_never_touch_the_original_checkout() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: touches-a-file
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );

    // The default (`worktree`) isolation must never let the agent's edit
    // land in the checkout the user is looking at.
    assert!(!repo.join("made.txt").exists());
    // It lives in a dedicated worktree under the state root instead.
    let worktrees_dir = home.join("worktrees");
    let made_somewhere = std::fs::read_dir(&worktrees_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|entry| entry.path().join("made.txt").exists());
    assert!(
        made_somewhere,
        "expected made.txt inside some worktree under {}",
        worktrees_dir.display()
    );
}

#[test]
fn two_runs_on_the_same_repo_get_independent_worktrees() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: touches-a-file
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
"#,
    );

    let run1 = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run1.status.success(), "run1: {}", stdout(&run1));
    let run2 = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run2.status.success(), "run2: {}", stdout(&run2));

    let worktree_dirs: Vec<_> = std::fs::read_dir(home.join("worktrees"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        worktree_dirs.len(),
        2,
        "expected one dedicated worktree per run"
    );
}

#[test]
fn isolation_none_refuses_a_dirty_tree_before_creating_any_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: needs-clean-tree
nodes:
  - id: touch
    kind: bash
    run: "true"
"#,
    );
    // Dirty the tree.
    write(&repo.join("uncommitted.txt"), "dirty");

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    assert!(
        !home.join("runs").exists(),
        "no run must be created when isolation refuses the dirty tree"
    );
}

#[test]
fn isolation_none_operates_directly_on_the_checkout_and_releases_its_lock_when_finished() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: no-op
nodes:
  - id: noop
    kind: bash
    run: "true"
"#,
    );
    // Isolation `none` requires a clean tree — commit the fixtures
    // themselves so only the workflow's own effects could dirty it.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run1 = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run1.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run1),
        String::from_utf8_lossy(&run1.stderr)
    );
    // Finishing must release the lock so a second run on the same
    // (still clean) checkout can proceed.
    let run2 = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run2.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run2),
        String::from_utf8_lossy(&run2.stderr)
    );
    // `none` never creates a dedicated worktree.
    assert!(!home.join("worktrees").exists());
}

#[test]
fn resuming_a_paused_run_continues_in_the_same_worktree_the_run_created() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: pauses-on-a-failing-node
nodes:
  - id: setup
    kind: bash
    run: "echo hello > marker.txt"
  - id: always-fails
    kind: bash
    depends_on: [setup]
    run: "touch attempt-marker.txt && exit 1"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(stdout(&run).contains("paused"), "got: {}", stdout(&run));
    let run_id = run_id_from(&run);

    let resume = yunta_in(&repo, &home, &["resume", &run_id]);
    assert!(
        stdout(&resume).contains("paused"),
        "got: {}",
        stdout(&resume)
    );

    // Resume must operate on the very worktree `run` created — never a
    // second one, and never the original checkout.
    assert!(!repo.join("attempt-marker.txt").exists());
    let worktree_dirs: Vec<_> = std::fs::read_dir(home.join("worktrees"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        worktree_dirs.len(),
        1,
        "resume must reuse the run's own worktree, not create another"
    );
    assert!(worktree_dirs[0].path().join("marker.txt").exists());
    assert!(worktree_dirs[0].path().join("attempt-marker.txt").exists());
}

#[test]
fn a_parallel_group_with_undeclared_scope_warns_but_the_run_still_completes() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
        kind: bash
        run: "touch load.txt"
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
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("pre-launch") && stderr.contains("warning"),
        "expected a D100 warning naming the group, got: {stderr}"
    );
}

// --- T1.5: `--input` (§2.3, D82) --------------------------------------------

#[test]
fn an_input_s_default_is_used_when_input_is_not_given_on_the_command_line() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: greet
inputs:
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "test '{{inputs.greeting}}' = 'hola'"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn an_explicit_input_flag_overrides_the_default() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: greet
inputs:
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "test '{{inputs.greeting}}' = 'bonjour'"
"#,
    );

    let run = yunta_in(
        &repo,
        &home,
        &["run", "wf.yaml", "--input", "greeting=bonjour"],
    );
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn a_missing_required_input_refuses_before_creating_any_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: needs-idea
inputs:
  idea:
    type: string
    required: true
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("idea"), "got: {stderr}");
    assert!(
        !home.join("runs").exists(),
        "no run must be created when input resolution fails"
    );
}

#[test]
fn mode_is_refused_since_modes_have_no_schema_yet() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml", "--mode", "ship"]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("mode"), "got: {stderr}");
}

// --- T7.1: list, doctor, gc, cancel -----------------------------------------

#[test]
fn list_shows_workflows_under_the_repo_s_own_directory_with_their_inputs() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/greet.yaml"),
        r#"
name: greet
description: "Says hello"
inputs:
  greeting:
    type: string
    default: hola
    description: "What to say"
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
    );

    let list = yunta_in(&repo, &home, &["list"]);
    assert!(list.status.success());
    let text = stdout(&list);
    assert!(text.contains("greet: Says hello"), "got: {text}");
    assert!(text.contains("greeting"), "got: {text}");
    assert!(text.contains("optional"), "got: {text}");
}

#[test]
fn list_runs_shows_local_runs_with_their_progress_summary() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let list = yunta_in(&repo, &home, &["list", "--runs"]);
    assert!(list.status.success());
    let text = stdout(&list);
    assert!(text.contains(&run_id), "got: {text}");
    assert!(text.contains("nodes"), "got: {text}");
    assert!(text.contains("finished"), "got: {text}");
}

#[test]
fn doctor_reports_no_adapter_when_runners_names_none_this_build_supports() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let doctor = yunta_in(&repo, &home, &["doctor"]);
    assert!(doctor.status.success());
    assert!(stdout(&doctor).contains("no adapter to probe"));
}

#[test]
fn gc_does_nothing_when_retention_days_is_not_configured() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let gc = yunta_in(&repo, &home, &["gc"]);
    assert!(gc.status.success());
    assert!(stdout(&gc).contains("retention_days"));
}

#[test]
fn gc_reclaims_a_finished_run_past_its_retention_window() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);
    assert!(run_dir.exists());

    let gc = yunta_in(&repo, &home, &["gc"]);
    assert!(
        gc.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&gc.stderr)
    );
    assert!(stdout(&gc).contains("reclaimed"), "got: {}", stdout(&gc));
    assert!(!run_dir.exists(), "run.dir should have been removed");
}

#[test]
fn gc_dry_run_reports_without_removing_anything() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);

    let gc = yunta_in(&repo, &home, &["gc", "--dry-run"]);
    assert!(gc.status.success());
    assert!(
        stdout(&gc).contains("would be reclaimed"),
        "got: {}",
        stdout(&gc)
    );
    assert!(run_dir.exists(), "dry-run must never remove anything");
}

#[test]
fn cancel_on_an_already_finished_run_is_a_clean_no_op() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let cancel = yunta_in(&repo, &home, &["cancel", &run_id]);
    assert!(cancel.status.success());
    assert!(stdout(&cancel).contains("nothing to cancel"));
}

#[test]
fn status_shows_the_normative_counters_with_context_summary() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: two-nodes
nodes:
  - id: a
    kind: bash
    run: "true"
  - id: b
    kind: bash
    depends_on: [a]
    run: "true"
"#,
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let status = yunta_in(&repo, &home, &["status", &run_id]);
    assert!(status.status.success());
    let text = stdout(&status);
    assert!(text.contains("2/2 nodes"), "got: {text}");
    assert!(text.contains("0 reroutes"), "got: {text}");
    assert!(text.contains("finished"), "got: {text}");
}

#[test]
fn a_gate_with_stdin_not_a_tty_pauses_instead_of_hanging() {
    // T7.2/§4.1: "sin TTY... nunca cuelga" — a `yunta run` whose stdin
    // isn't a terminal (exactly `cargo test`'s own usual case, made
    // explicit here with `Stdio::null()` so this doesn't depend on
    // whatever stdin the test binary itself happened to inherit) must
    // degrade to pausing at a gate, never sit waiting for a keystroke
    // nobody can send it.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f never-created.txt"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: bash
    depends_on: []
    run: "true"
"#,
    );

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to run the yunta binary");

    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(text.contains("paused"), "got: {text}");
}

#[test]
fn run_follow_prints_progress_while_the_run_is_still_in_progress() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // Long enough that the 500ms poller in `spawn_follower` gets at
    // least one tick in before the node (and so the run) finishes.
    write(
        &repo.join("wf.yaml"),
        "name: slow\nnodes:\n  - id: only\n    kind: bash\n    run: \"sleep 1.2\"\n",
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml", "--follow"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    let text = stdout(&run);
    assert!(
        text.contains("0/1 nodes") && text.contains("running"),
        "expected at least one in-progress follow line, got: {text}"
    );
}

#[test]
fn a_second_run_over_max_concurrent_runs_is_refused_while_one_is_paused() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "limits:\n  max_concurrent_runs: 1\n",
    );
    // A bash node that fails with no on_failure pauses the run, leaving
    // it non-terminal.
    write(
        &repo.join("wf.yaml"),
        "name: pauser\nnodes:\n  - id: boom\n    kind: bash\n    run: \"false\"\n",
    );

    // A paused run exits non-zero (it needs attention) but leaves its
    // slot occupied — that's the state the second invocation must see.
    let first = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(stdout(&first).contains("paused"), "got: {}", stdout(&first));

    let second = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        !second.status.success(),
        "the second run must be refused while the first is paused"
    );
    let err = String::from_utf8_lossy(&second.stderr);
    assert!(
        err.contains("max_concurrent_runs"),
        "the refusal must name the limit: {err}"
    );
    assert!(
        err.contains("resume") || err.contains("cancel"),
        "the refusal must say what to do about it: {err}"
    );
}

#[test]
fn a_finished_run_never_counts_against_max_concurrent_runs() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "limits:\n  max_concurrent_runs: 1\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: ok\nnodes:\n  - id: fine\n    kind: bash\n    run: \"true\"\n",
    );

    let first = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(first.status.success());
    assert!(stdout(&first).contains("finished"));

    let second = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        second.status.success(),
        "a finished run holds no slot: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}

#[test]
fn yunta_verify_reports_an_untouched_run_s_chain_intact() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: chain\nnodes:\n  - id: fine\n    kind: bash\n    run: \"true\"\n",
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let verify = yunta_in(&repo, &home, &["verify", &run_id]);
    assert!(
        verify.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert!(
        stdout(&verify).contains("intact"),
        "got: {}",
        stdout(&verify)
    );

    let ghost = yunta_in(&repo, &home, &["verify", "run-ghost"]);
    assert!(!ghost.status.success(), "an unknown run must not verify");
}

#[test]
fn a_live_run_registers_its_processes_in_engine_json_and_deletes_it_at_terminal() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // The bash node is itself a registered process group — it captures
    // the registry mid-run from inside the run. The short sleep lets the
    // engine's registration (which happens right after spawn, while the
    // command already runs) land first.
    write(
        &repo.join("wf.yaml"),
        r#"
name: registry
nodes:
  - id: capture
    kind: bash
    run: "sleep 0.2 && cp {{run.dir}}/scratch/engine.json {{run.dir}}/scratch/captured.json"
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);

    let captured = std::fs::read_to_string(run_dir.join("scratch/captured.json"))
        .expect("the bash node must have seen engine.json while it ran");
    let parsed: serde_json::Value = serde_json::from_str(&captured).unwrap();
    assert!(
        parsed["engine_pid"].as_u64().unwrap_or(0) > 0,
        "got: {captured}"
    );
    assert!(
        !parsed["process_groups"].as_array().unwrap().is_empty(),
        "the bash node's own process group must be registered: {captured}"
    );

    assert!(
        !run_dir.join("scratch/engine.json").exists(),
        "engine.json must be deleted once the run reaches a terminal"
    );
}

#[test]
fn ctrl_c_pauses_the_run_kills_the_process_tree_and_releases_the_none_lock() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    // The child ignores SIGINT on purpose (the T3.3 pattern): only the
    // engine's interrupt→kill escalation can take it down, which is
    // exactly what this proves.
    write(
        &repo.join("wf.yaml"),
        r#"
name: stubborn
nodes:
  - id: stubborn
    kind: bash
    run: "echo $$ > child.pid; trap '' INT; sleep 30"
"#,
    );
    // Isolation `none` requires a clean tree — commit the fixtures.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let mut yunta = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // Wait for the bash node to actually start (it writes its pid).
    let pid_path = repo.join("child.pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !pid_path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the bash node never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let child_pid = std::fs::read_to_string(&pid_path)
        .unwrap()
        .trim()
        .to_string();

    // Simulated Ctrl-C: SIGINT to the yunta process.
    let killed = std::process::Command::new("kill")
        .args(["-INT", &yunta.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());

    let output = yunta.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("cancelled by user"),
        "the pause must say why: {text}"
    );

    // Zero zombies: the SIGINT-ignoring child is dead anyway.
    let alive = std::process::Command::new("kill")
        .args(["-0", &child_pid])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!alive.success(), "the stubborn child must be dead");

    // The `none` lock is released, and engine.json is gone.
    assert!(!repo.join(".git/yunta-none.lock").exists());
    let run_id = run_id_from(&output);
    assert!(!home
        .join("runs")
        .join(&run_id)
        .join("scratch/engine.json")
        .exists());
}

/// Spawns `yunta run` detached and waits until the given file exists —
/// the bash node's own signal that it is really running.
fn spawn_run_until(repo: &Path, home: &Path, marker: &Path) -> std::process::Child {
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(repo)
        .env("YUNTA_HOME", home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the bash node never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    child
}

/// The one run id under `home/runs` — usable before the run's own
/// process has printed anything.
fn only_run_id(home: &Path) -> String {
    let mut entries: Vec<String> = std::fs::read_dir(home.join("runs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one run: {entries:?}");
    entries.pop().unwrap()
}

#[test]
fn yunta_cancel_stops_a_live_run_from_a_separate_process() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: long
nodes:
  - id: long
    kind: bash
    run: "echo $$ > child.pid; sleep 30"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let mut yunta = spawn_run_until(&repo, &home, &repo.join("child.pid"));
    let run_id = only_run_id(&home);

    let cancel = yunta_in(&repo, &home, &["cancel", &run_id]);
    assert!(
        cancel.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&cancel),
        String::from_utf8_lossy(&cancel.stderr)
    );
    assert!(
        stdout(&cancel).contains("cancelled"),
        "got: {}",
        stdout(&cancel)
    );

    // The run process exits, its child is dead, the log is terminal.
    let output = yunta.wait_with_output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("cancelled by user"));
    let child_pid = std::fs::read_to_string(repo.join("child.pid"))
        .unwrap()
        .trim()
        .to_string();
    let alive = std::process::Command::new("kill")
        .args(["-0", &child_pid])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!alive.success(), "the sleeping child must be dead");

    let status = yunta_in(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("cancelled by user"),
        "got: {}",
        stdout(&status)
    );
}

#[test]
fn yunta_cancel_cleans_up_after_a_crashed_engine() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: crashy
nodes:
  - id: long
    kind: bash
    run: "echo $$ > child.pid; sleep 30"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let mut yunta = spawn_run_until(&repo, &home, &repo.join("child.pid"));
    let run_id = only_run_id(&home);

    // Simulated crash: SIGKILL gives the engine no chance to clean up —
    // engine.json survives with the orphaned process group in it.
    let killed = std::process::Command::new("kill")
        .args(["-KILL", &yunta.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let _ = yunta.wait();
    let engine_json = home.join("runs").join(&run_id).join("scratch/engine.json");
    assert!(
        engine_json.exists(),
        "the crash must leave engine.json behind"
    );

    let cancel = yunta_in(&repo, &home, &["cancel", &run_id]);
    assert!(
        cancel.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    assert!(
        stdout(&cancel).contains("already dead"),
        "got: {}",
        stdout(&cancel)
    );

    // The orphaned child is dead, the registry is gone, the log records
    // the crash-cancellation. `kill -0` succeeds on a zombie (the
    // SIGKILLed engine never reaped it), so check the process state:
    // gone or Z both mean the kill landed.
    let child_pid = std::fs::read_to_string(repo.join("child.pid"))
        .unwrap()
        .trim()
        .to_string();
    let state = std::process::Command::new("ps")
        .args(["-o", "state=", "-p", &child_pid])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&state.stdout).trim().to_string();
    assert!(
        state.is_empty() || state.starts_with('Z'),
        "the orphaned child must be dead, ps state: {state}"
    );
    assert!(!engine_json.exists());
    let status = yunta_in(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("cancelled after crash"),
        "got: {}",
        stdout(&status)
    );
}

#[test]
fn resume_uses_the_worktree_frozen_in_the_manifest_after_a_paths_change() {
    // The T2.4 acceptance criterion, executed: create a run, change
    // `paths.worktrees`, and `resume` completes in the worktree the run
    // was born with — not wherever the config points today.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: frozen-paths
nodes:
  - id: gated
    kind: bash
    run: "echo x > started.txt; test -f go.txt || sleep 30"
"#,
    );

    // Crash the engine mid-node (the node sleeps until go.txt exists).
    let mut yunta = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let worktree = loop {
        assert!(
            std::time::Instant::now() < deadline,
            "the node never started"
        );
        if let Ok(entries) = std::fs::read_dir(home.join("worktrees")) {
            if let Some(entry) = entries.flatten().next() {
                if entry.path().join("started.txt").exists() {
                    break entry.path();
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let run_id = only_run_id(&home);
    std::process::Command::new("kill")
        .args(["-KILL", &yunta.id().to_string()])
        .status()
        .unwrap();
    let _ = yunta.wait();

    // The condition the restarted node needs, in the ORIGINAL worktree —
    // then move the config's worktrees root somewhere else entirely.
    write(&worktree.join("go.txt"), "go");
    write(
        &repo.join(".yunta/config.yaml"),
        "paths:\n  worktrees: elsewhere-worktrees\n",
    );

    let resume = yunta_in(&repo, &home, &["resume", &run_id]);
    assert!(
        resume.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&resume),
        String::from_utf8_lossy(&resume.stderr)
    );
    assert!(
        stdout(&resume).contains("finished"),
        "got: {}",
        stdout(&resume)
    );
}

#[test]
fn a_manifest_without_frozen_paths_still_resumes_via_the_current_config() {
    // Tolerant reader (D70): a pre-DI-07 manifest (no `paths:` block)
    // resumes exactly as before, from the current config's roots.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: legacy\nnodes:\n  - id: fine\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    // Strip the frozen paths block, simulating an old manifest.
    let manifest_path = home.join("runs").join(&run_id).join("manifest.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    let stripped: String = {
        let mut out = String::new();
        let mut in_paths = false;
        for line in manifest.lines() {
            if line.starts_with("paths:") {
                in_paths = true;
                continue;
            }
            if in_paths && line.starts_with(' ') {
                continue;
            }
            in_paths = false;
            out.push_str(line);
            out.push('\n');
        }
        out
    };
    assert_ne!(manifest, stripped, "the paths block must have been there");
    std::fs::write(&manifest_path, stripped).unwrap();

    let resume = yunta_in(&repo, &home, &["resume", &run_id]);
    assert!(
        resume.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resume.stderr)
    );
    assert!(stdout(&resume).contains("finished"));
}

#[test]
fn a_cancelled_run_resumes_by_restarting_the_orphaned_node() {
    // DI-11/§8.1: a user cancellation leaves the interrupted node
    // orphaned — no fabricated terminal — so `resume` re-treats it per
    // `on_interrupt` (restart_node) instead of dead-ending on a failed
    // node.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: resumable
nodes:
  - id: gated
    kind: bash
    run: "echo x > started.txt; test -f go.txt || sleep 30"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let mut yunta = spawn_run_until(&repo, &home, &repo.join("started.txt"));
    let run_id = only_run_id(&home);

    std::process::Command::new("kill")
        .args(["-INT", &yunta.id().to_string()])
        .status()
        .unwrap();
    let output = yunta.wait_with_output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("cancelled by user"),
        "got: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    // Give the restarted node its exit condition, then resume.
    write(&repo.join("go.txt"), "go");
    let resume = yunta_in(&repo, &home, &["resume", &run_id]);
    assert!(
        resume.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&resume),
        String::from_utf8_lossy(&resume.stderr)
    );
    assert!(
        stdout(&resume).contains("finished"),
        "got: {}",
        stdout(&resume)
    );
}

#[test]
fn on_finish_cleanup_removes_the_worktree_and_keeps_the_run_dir() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: tidy
nodes:
  - id: fine
    kind: bash
    run: "true"
on_finish:
  - cleanup: worktree
"#,
    );

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_id = run_id_from(&run);

    assert!(
        !home.join("worktrees").join(&run_id).exists(),
        "the declared cleanup must remove the run's worktree"
    );
    assert!(
        home.join("runs")
            .join(&run_id)
            .join("manifest.yaml")
            .exists(),
        "run.dir stays — cleanup is about the checkout, never the audit trail"
    );
    // The run branch pointed at the base commit (nothing was committed),
    // so `git branch -d` agreed to delete it.
    let branches = std::process::Command::new("git")
        .args(["branch", "--list", &format!("yunta/{run_id}")])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&branches.stdout).trim().is_empty(),
        "a merged run branch is deleted"
    );
}

#[test]
fn distill_under_isolation_none_leaves_uncommitted_files_and_the_next_run_refuses() {
    // The engine never commits the user's own branch: distilled files
    // stay visible and uncommitted, and the next `none` run refuses the
    // dirty tree until a human commits or discards — deliberate
    // friction, not a bug.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: distill-none
nodes:
  - id: plan
    kind: bash
    run: "echo durable > {{run.dir}}/artifacts/plan.md"
    artifacts:
      produces: [plan.md]
on_finish:
  - distill: [plan.md]
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let distilled = repo.join(".yunta/knowledge/distilled/distill-none");
    assert!(distilled.exists(), "the files land in the user's checkout");
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status.stdout).contains(".yunta/knowledge"),
        "uncommitted — the engine never commits the user's branch"
    );

    let second = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        !second.status.success(),
        "the dirty tree must refuse the next `none` run"
    );
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("clean tree"),
        "got: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}

#[test]
fn gc_reclaims_files_first_and_purges_rows_only_on_a_later_pass() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "storage:\n  retention_days: 0\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: short\nnodes:\n  - id: fine\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);
    let run_dir = home.join("runs").join(&run_id);
    assert!(run_dir.exists());

    // Pass 1: files die, rows survive — the DB is never first to go.
    let first = yunta_in(&repo, &home, &["gc"]);
    assert!(first.status.success());
    assert!(!run_dir.exists(), "run.dir reclaimed: {}", stdout(&first));
    // The rows survive pass 1 — `verify` (which needs only the DB)
    // still walks the chain.
    let verify = yunta_in(&repo, &home, &["verify", &run_id]);
    assert!(
        verify.status.success() && stdout(&verify).contains("intact"),
        "rows still readable after pass 1: {}",
        String::from_utf8_lossy(&verify.stderr)
    );

    // Pass 2: the dir is gone, so the rows go now.
    let second = yunta_in(&repo, &home, &["gc"]);
    assert!(second.status.success());
    assert!(
        stdout(&second).contains("purged"),
        "got: {}",
        stdout(&second)
    );

    // A purged run reads back as unknown — never corrupt state.
    let status = yunta_in(&repo, &home, &["status", &run_id]);
    assert!(!status.status.success());
    let verify = yunta_in(&repo, &home, &["verify", &run_id]);
    assert!(!verify.status.success());
    assert!(
        String::from_utf8_lossy(&verify.stderr).contains("no events"),
        "got: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
}

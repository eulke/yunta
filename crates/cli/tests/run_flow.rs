//! End-to-end CLI flows: `yunta run` on a bash-only workflow (no agent
//! adapter needed), `status` over its log, `resume` idempotence,
//! `yunta test` driving a workflow with the mock adapter from a case
//! file (the designed home for mock fixtures), and `yunta run`
//! actually spawning the real `claude-code` adapter against a
//! scripted fake `claude` binary — no network, no cost.

use std::path::{Path, PathBuf};

use yunta_adapters::signal::{liveness, signal_group, signal_process, Liveness, Signal};
use yunta_core::Pid;
use yunta_testkit::{
    git, init_repo, run_id_from, stderr, stdout, wait_for, wait_until, write, yunta_in,
};

fn claude_code_stub() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../adapters/tests/fixtures/claude_code_stub.sh")
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"));

    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    assert!(
        status.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let state: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(state["summary"], "2/2 nodes · 0 reroutes · finished");
    assert_eq!(state["nodes"]["touch"], "finished — exit 0");
    assert_eq!(state["nodes"]["verify"], "finished — exit 0");

    // Resuming a finished run is a clean no-op.
    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
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
    // does build (`claude-code`; `codex`) are ruled out on
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr
            .lines()
            .any(|l| l == "a test case under .yunta/tests/ and run `yunta test`."),
        "the refusal's closing line directs the user to the mock adapter via `yunta test`: {stderr}"
    );
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

    let output = yunta_in!(&repo, &home, &["test"]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "stdout: {text}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        text.trim_end(),
        "case happy-path ... ok\n1 case, 0 failed",
        "the one case runs against the mock adapter and every expectation holds"
    );
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

    let output = yunta_in!(&repo, &home, &["test"]);
    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(
        text.lines()
            .any(|l| l == "case wrong-expect ... FAILED: 1 error"),
        "the verdict counts the problems listed under it: {text}"
    );
    // The words a case file is written with, not a Rust enum's `Debug`.
    assert!(
        text.lines()
            .any(|l| l == "  final_state: expected finished, got paused"),
        "the mismatch names the field, the expected state and the actual one: {text}"
    );
    // And the reason sits on its own line under it, unquoted and
    // unescaped — it was written for a reader already.
    assert!(
        text.lines()
            .any(|l| l.trim_start().starts_with("node `fails` failed:")),
        "the reason reaches the reader intact: {text}"
    );
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"), "got: {}", stdout(&run));
}

#[test]
fn an_adapter_that_reports_itself_unhealthy_without_saying_why_is_refused_by_name_alone() {
    // A CLI whose `--version` exits non-zero writing nothing is the
    // shape a probe has no diagnostic for. The refusal lists the
    // adapter and stops: a colon with nothing behind it would promise
    // the reader a reason the CLI never gave.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let silent = repo.join("silent-cli.sh");
    write(&silent, "#!/bin/sh\nexit 1\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&silent).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&silent, perms).unwrap();
    }

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
            binary = silent.display()
        ),
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: unhealthy-adapter
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    assert_eq!(
        stderr(&run).trim_end(),
        "error: adapter health check failed (run `yunta doctor` for detail): 1 error\n  \
         claude-code"
    );
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run1 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run1.status.success(), "run1: {}", stdout(&run1));
    let run2 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run1 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run1.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run1),
        String::from_utf8_lossy(&run1.stderr)
    );
    // Finishing must release the lock so a second run on the same
    // (still clean) checkout can proceed.
    let run2 = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(stdout(&run).contains("paused"), "got: {}", stdout(&run));
    let run_id = run_id_from(&run);

    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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
        "expected a scope-collision warning naming the group, got: {stderr}"
    );
}

// --- `--input` ---------------------------------------------------------

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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run = yunta_in!(
        &repo,
        &home,
        &["run", "wf.yaml", "--input", "greeting=bonjour"]
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stderr).trim_end(),
        "error: input `idea` is required and has no default — pass `--input idea=...`",
        "the refusal names the missing input and how to supply it"
    );
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--mode", "ship"]);
    assert!(!run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stderr).trim_end(),
        "error: workflow `only-node` declares no mode `ship` — declared modes: \
         (none — this workflow declares no modes:)",
        "the refusal names the unknown mode and that the workflow declares none"
    );
}

// --- list, doctor, cancel ----------------------------------------------

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

    let list = yunta_in!(&repo, &home, &["list"]);
    assert!(list.status.success());
    assert_eq!(
        stdout(&list).trim_end(),
        "greet: Says hello\n  --input greeting=... (string, optional) — What to say",
        "list shows the workflow, its description, and its one optional input"
    );
}

#[test]
fn list_runs_groups_a_run_under_what_can_be_done_about_it() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let list = yunta_in!(&repo, &home, &["list", "--runs"]);
    assert!(list.status.success());
    let text = stdout(&list);
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("closed (1)"),
        "a finished run is listed under what can be done about it: {text}"
    );
    let row = lines.next().unwrap_or_default();
    assert!(
        row.starts_with(&format!("  {run_id}  only-node (default)")),
        "the row names the workflow and the mode, not only the id: {text}"
    );
    assert_eq!(
        lines.next(),
        Some("    1/1 nodes · 0 reroutes · finished"),
        "under it, the same summary `yunta status` prints: {text}"
    );
}

#[test]
fn doctor_reports_no_adapter_when_runners_names_none_this_build_supports() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let doctor = yunta_in!(&repo, &home, &["doctor"]);
    assert!(doctor.status.success());
    assert!(stdout(&doctor).contains("no adapter to probe"));
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
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let cancel = yunta_in!(&repo, &home, &["cancel", &run_id]);
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
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    assert!(
        status.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let state: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        state["summary"], "2/2 nodes · 0 reroutes · finished",
        "the normative counters read both nodes done, no reroutes, finished"
    );
}

#[test]
fn a_mode_that_includes_a_parallel_group_counts_its_children_inside_it() {
    // `modes:` names top-level nodes, and a group the mode schedules
    // runs every child it declares. The denominator is therefore the
    // group plus its children, and the only skipped node is the
    // top-level one the mode leaves out.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: fan-out
modes:
  fan-only: { include: [fan] }
  full: { include: all }
nodes:
  - id: fan
    kind: parallel
    join: all
    nodes:
      - { id: left, kind: bash, run: "true" }
      - { id: right, kind: bash, run: "true" }
  - { id: audit, kind: bash, run: "true" }
"#,
    );
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--mode", "fan-only"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    let run_id = run_id_from(&run);

    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        state["summary"],
        "3/3 nodes \u{b7} 1 skipped (mode: fan-only) \u{b7} 0 reroutes \u{b7} finished",
        "the group and both its children are inside the denominator, and the \
         only skipped node is the top-level one the mode leaves out"
    );
}

#[test]
fn a_gate_with_stdin_not_a_tty_pauses_instead_of_hanging() {
    // "sin TTY... nunca cuelga" — a `yunta run` whose stdin
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
    let run_id = run_id_from(&output);
    assert!(
        text.lines()
            .any(|l| l.starts_with(&format!("run {run_id}: ")) && l.contains("paused")),
        "a gate with no TTY degrades to a paused run instead of hanging: {text}"
    );
    assert!(
        text.contains(&format!("yunta resolve-gate {run_id} <option>")),
        "and says how to answer it from anywhere: {text}"
    );
}

#[test]
fn progress_reaches_the_reader_while_the_run_is_still_in_progress() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // The node blocks after announcing itself, so every assertion below
    // is made while the run is genuinely mid-flight — no interval to
    // outlast, because the engine hands each event to the surface as it
    // writes it.
    // `isolation: none` runs the node in this checkout, so the test reads
    // the marker where the node writes it.
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: paced
nodes:
  - id: first
    kind: bash
    run: "echo running > first.started; tail -f /dev/null"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    // Outside the repo: `isolation: none` demands a clean tree.
    let progress_log = root.path().join("progress.log");
    let mut run = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::fs::File::create(&progress_log).unwrap())
        .spawn()
        .unwrap();

    wait_until(
        || marker_written(&repo.join("first.started")),
        || "the bash node never started".into(),
    );

    // Progress is on stderr while the node is still blocked: the run has
    // not finished, and the reader already knows which node holds it.
    let progress = wait_for(
        || {
            let text = std::fs::read_to_string(&progress_log).unwrap_or_default();
            text.contains("first").then_some(text)
        },
        || {
            format!(
                "no progress named the running node, got: {}",
                std::fs::read_to_string(&progress_log).unwrap_or_default()
            )
        },
    );
    assert!(
        run.try_wait().unwrap().is_none(),
        "the run must still be in progress when its progress is read: {progress}"
    );

    // With stderr on a file rather than a terminal, line one says the
    // live view stood down and names what was missing.
    assert_eq!(
        progress.lines().next(),
        Some("live view off (stderr is not a terminal): one line per event"),
        "got: {progress}"
    );

    signal_process(pid_of(&run), Signal::SIGINT).expect("the run is alive to be interrupted");
    let output = run.wait_with_output().unwrap();
    assert!(
        stdout(&output).contains("cancelled"),
        "the closing block names the outcome: {}",
        stdout(&output)
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
    let first = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(stdout(&first).contains("paused"), "got: {}", stdout(&first));

    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let first = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(first.status.success());
    assert!(stdout(&first).contains("finished"));

    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());
    let run_id = run_id_from(&run);

    let verify = yunta_in!(&repo, &home, &["verify", &run_id]);
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

    let ghost = yunta_in!(&repo, &home, &["verify", "run-ghost"]);
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
    // the registry mid-run from inside the run. The engine writes the
    // registry before this node exists and adds this node's own group
    // right after spawning it, so the node polls for the registration
    // and not for the file: waiting on the file alone is satisfied the
    // instant the node starts, and copies a registry that does not name
    // it yet. The bound is what turns a registration that never lands
    // into the failed assertion below rather than a hung test.
    write(
        &repo.join("wf.yaml"),
        r#"
name: registry
nodes:
  - id: capture
    kind: bash
    run: |
      registry={{run.dir}}/scratch/engine.json
      attempts=0
      while [ "$attempts" -lt 2000 ]; do
        if [ -s "$registry" ] && ! grep -q '"process_groups": \[\]' "$registry"; then
          break
        fi
        attempts=$((attempts + 1))
      done
      cp "$registry" {{run.dir}}/scratch/captured.json
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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
    // The child ignores SIGINT on purpose: only the
    // engine's interrupt→kill escalation can take it down, which is
    // exactly what this proves.
    write(
        &repo.join("wf.yaml"),
        r#"
name: stubborn
nodes:
  - id: stubborn
    kind: bash
    run: "echo $$ > child.pid; trap '' INT; tail -f /dev/null"
"#,
    );
    // Isolation `none` requires a clean tree — commit the fixtures.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let yunta = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // Wait for the bash node to actually start and write its pid — poll for
    // the pid content, not just the file, so a tight loop never reads it
    // half-written.
    let pid_path = repo.join("child.pid");
    wait_until(
        || marker_written(&pid_path),
        || "the bash node never wrote its pid".into(),
    );
    let child_pid = std::fs::read_to_string(&pid_path)
        .unwrap()
        .trim()
        .to_string();

    // Simulated Ctrl-C: SIGINT to the yunta process.
    signal_process(pid_of(&yunta), Signal::SIGINT).expect("yunta is alive to be interrupted");

    let output = yunta.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("cancelled by user"),
        "the pause must say why: {text}"
    );

    // Zero zombies: the SIGINT-ignoring child is dead anyway.
    assert_eq!(
        liveness(parse_pid(&child_pid)),
        Liveness::Dead,
        "the stubborn child must be dead"
    );

    // The `none` lock is released, and engine.json is gone.
    assert!(!repo.join(".git/yunta-none.lock").exists());
    let run_id = run_id_from(&output);
    assert!(!home
        .join("runs")
        .join(&run_id)
        .join("scratch/engine.json")
        .exists());
}

/// True once `marker` exists and holds non-whitespace content — the point
/// at which the shell has finished writing it, not merely created it. A
/// poll for existence alone can observe the file in the window between
/// creation and the write completing.
fn marker_written(marker: &Path) -> bool {
    std::fs::read_to_string(marker)
        .map(|content| !content.trim().is_empty())
        .unwrap_or(false)
}

/// Spawns `yunta run` detached and waits until the given file is written —
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
    wait_until(
        || marker_written(marker),
        || "the bash node never started".into(),
    );
    child
}

/// The process state `ps` reports for `pid`: empty once the process is
/// gone, `Z` while it is a zombie nobody reaped.
fn process_state(pid: &str) -> String {
    let output = std::process::Command::new("ps")
        .args(["-o", "state=", "-p", pid])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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
    run: "echo $$ > child.pid; tail -f /dev/null"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let yunta = spawn_run_until(&repo, &home, &repo.join("child.pid"));
    let run_id = only_run_id(&home);

    let cancel = yunta_in!(&repo, &home, &["cancel", &run_id]);
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
    assert_eq!(
        liveness(parse_pid(&child_pid)),
        Liveness::Dead,
        "the blocked child must be dead"
    );

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
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
    run: "echo $$ > child.pid; tail -f /dev/null"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let mut yunta = spawn_run_until(&repo, &home, &repo.join("child.pid"));
    let run_id = only_run_id(&home);

    // Simulated crash: SIGKILL gives the engine no chance to clean up —
    // engine.json survives with the orphaned process group in it.
    signal_process(pid_of(&yunta), Signal::SIGKILL).expect("yunta is alive to be killed");
    let _ = yunta.wait();
    let engine_json = home.join("runs").join(&run_id).join("scratch/engine.json");
    assert!(
        engine_json.exists(),
        "the crash must leave engine.json behind"
    );

    let cancel = yunta_in!(&repo, &home, &["cancel", &run_id]);
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
    // gone or Z both mean the kill landed. `cancel` returns once the kill
    // is sent; the child leaves the process table on the kernel's
    // schedule, not ours.
    let child_pid = std::fs::read_to_string(repo.join("child.pid"))
        .unwrap()
        .trim()
        .to_string();
    let dead = |state: &str| state.is_empty() || state.starts_with('Z');
    wait_until(
        || dead(&process_state(&child_pid)),
        || {
            format!(
                "the orphaned child must be dead, ps state: {}",
                process_state(&child_pid)
            )
        },
    );
    assert!(!engine_json.exists());
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("cancelled after crash"),
        "got: {}",
        stdout(&status)
    );
}

#[test]
fn resume_uses_the_worktree_frozen_in_the_manifest_after_a_paths_change() {
    // Create a run, change
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
    run: "echo x > started.txt; test -f go.txt || tail -f /dev/null"
"#,
    );

    // Crash the engine mid-node (the node blocks until go.txt exists).
    let mut yunta = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let worktree = wait_for(
        || {
            let entry = std::fs::read_dir(home.join("worktrees"))
                .ok()?
                .flatten()
                .next()?;
            entry
                .path()
                .join("started.txt")
                .exists()
                .then(|| entry.path())
        },
        || "the node never started".into(),
    );
    let run_id = only_run_id(&home);
    signal_process(pid_of(&yunta), Signal::SIGKILL).expect("yunta is alive to be killed");
    let _ = yunta.wait();

    // The condition the restarted node needs, in the ORIGINAL worktree —
    // then move the config's worktrees root somewhere else entirely.
    write(&worktree.join("go.txt"), "go");
    write(
        &repo.join(".yunta/config.yaml"),
        "paths:\n  worktrees: elsewhere-worktrees\n",
    );

    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
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
    // Tolerant reader: a manifest with no `paths:` block
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
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
    assert!(
        resume.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resume.stderr)
    );
    assert!(stdout(&resume).contains("finished"));
}

#[test]
fn frozen_paths_are_absolute_or_run_creation_fails() {
    // A run freezes its state roots so `resume`/`status`/`gc` find it from
    // any directory. A relative `paths.runs` would resolve against whatever
    // cwd a later reader happened to have, so run creation refuses it and
    // names the path — never silently rooting the run wherever `yunta run`
    // was invoked.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "paths:\n  runs: relative-runs\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: wf\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        !run.status.success(),
        "a relative state root must fail run creation, not be silently rooted at cwd: {}",
        stdout(&run)
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("relative-runs"),
        "the error must name the offending path: {stderr}"
    );
    assert!(
        stderr.contains("absolute"),
        "the error must say the root has to be absolute: {stderr}"
    );
    // The rejection happens before any run is created — the relative root is
    // never brought into being under the invocation directory.
    assert!(
        !repo.join("relative-runs").exists(),
        "a rejected root must not be created"
    );
}

#[test]
fn a_cancelled_run_resumes_by_restarting_the_orphaned_node() {
    // A user cancellation leaves the interrupted node
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
    run: "echo x > started.txt; test -f go.txt || tail -f /dev/null"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let yunta = spawn_run_until(&repo, &home, &repo.join("started.txt"));
    let run_id = only_run_id(&home);

    signal_process(pid_of(&yunta), Signal::SIGINT).expect("yunta is alive to be interrupted");
    let output = yunta.wait_with_output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("cancelled by user"),
        "got: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    // Give the restarted node its exit condition, then resume.
    write(&repo.join("go.txt"), "go");
    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
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

// --- `kind: workflow` composition from the CLI --------------------------

#[test]
fn a_composed_workflow_runs_from_the_cli_creating_a_linked_child_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // The child lives in the repo's versioned catalog — that's what
    // `use:` resolves against at child birth.
    write(
        &repo.join(".yunta/workflows/child.yaml"),
        r#"
name: child
nodes:
  - id: work
    kind: bash
    run: "echo from-child > child.txt"
"#,
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: child
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "catalog"]);

    let output = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let parent_id = run_id_from(&output);
    assert!(stdout(&output).contains("finished"));

    // The child is a complete run of its own under the same state root:
    // frozen manifest, own worktree with the work done. Its id is the
    // log's to give — the only other run directory is the child's.
    let child_id = std::fs::read_dir(home.join("runs"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .find(|name| *name != parent_id)
        .expect("the child run has a directory of its own");
    let child_manifest = home.join("runs").join(&child_id).join("manifest.yaml");
    assert!(
        child_manifest.exists(),
        "expected the child's frozen manifest at {}",
        child_manifest.display()
    );
    let child_tree = home.join("worktrees").join(&child_id);
    assert_eq!(
        std::fs::read_to_string(child_tree.join("child.txt"))
            .unwrap()
            .trim(),
        "from-child"
    );
}

#[test]
fn yunta_check_refuses_a_composition_cycle() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/a.yaml"),
        "name: a\nnodes:\n  - { id: sub, kind: workflow, use: b }\n",
    );
    write(
        &repo.join(".yunta/workflows/b.yaml"),
        "name: b\nnodes:\n  - { id: sub, kind: workflow, use: a }\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: parent\nnodes:\n  - { id: top, kind: workflow, use: a }\n",
    );

    let output = yunta_in!(&repo, &home, &["check", "wf.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("workflow composition cycle: a -> b -> a"),
        "stderr: {stderr}"
    );
}

/// A `kind: bash` node whose command fails saying nothing — `test -f x`,
/// the ordinary case — is reported by its exit code alone. A `:` promises
/// a reader that something follows it, and every surface that quotes the
/// diagnostic quotes that promise too.
#[test]
fn a_bash_node_that_fails_without_writing_to_stderr_is_reported_by_its_exit_code_alone() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: silent-failure
nodes:
  - id: verify
    kind: bash
    run: "test -f never-written.txt"
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        state["nodes"]["verify"], "failed — exit 1",
        "a command that said nothing is quoted with nothing promised: {state:#}"
    );
}

// --- `yunta run --detach` ------------------------------------------------

/// A fixture scripts the sessions of whoever runs them, and `--detach`
/// makes that a separate `yunta resume` — which resolves the adapters
/// `runners:` names and reads no fixture at all. The combination is
/// refused with what to do instead, never honoured as a run that quietly
/// stayed attached.
#[test]
fn detaching_a_run_against_a_mock_fixture_is_refused_before_anything_is_created() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // The workflow passes `yunta check` and the fixture completes it, so
    // the refusal is the only thing standing between this invocation and
    // a finished run.
    write(
        &repo.join(".yunta/config.yaml"),
        "runners:\n  executor:\n    - { adapter: claude-code, model: claude-model }\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: mocked
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );
    write(
        &repo.join("fixture.yaml"),
        "sessions:\n  - outcome: { type: completed, summary: done }\n",
    );

    for args in [
        vec![
            "run",
            "wf.yaml",
            "--detach",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
        ],
        // The same rule, reached with the fixture still missing: the
        // answer names the combination that cannot work rather than
        // sending the reader to add a flag that is refused next.
        vec!["run", "wf.yaml", "--detach", "--adapter", "mock"],
    ] {
        let refused = yunta_in!(&repo, &home, &args);
        assert!(
            !refused.status.success(),
            "`{args:?}` must be refused, got: {}",
            stdout(&refused)
        );
        let stderr = stderr(&refused);
        assert!(
            stderr.contains("--detach") && stderr.contains("yunta resume"),
            "the refusal names the flag and why a detached child cannot honour it: {stderr}"
        );
        assert!(
            !home.join("runs").exists(),
            "nothing is created for an invocation that is refused up front"
        );
    }
}

#[test]
fn yunta_run_detach_returns_immediately_and_the_workflow_finishes_in_a_detached_child() {
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
name: slow
nodes:
  - id: work
    kind: bash
    run: "until [ -f go.txt ]; do :; done; echo done > done.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach"]);

    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    // --detach returned even though the node blocks until `go.txt` appears:
    // had it waited for the workflow it would still be hanging now. The run
    // is in progress, not finished, at the moment the invocation returns.
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        !stdout(&status).contains("finished"),
        "the workflow is still running in the detached child: {}",
        stdout(&status)
    );

    // Release the node and watch the detached child carry the run to
    // completion on its own.
    write(&repo.join("go.txt"), "go");
    let status = || stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    wait_until(
        || status().contains("finished"),
        || format!("the detached run never reached finished: {}", status()),
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("done.txt"))
            .unwrap()
            .trim(),
        "done"
    );
}

#[test]
fn yunta_run_detach_survives_a_sigterm_to_its_own_launchers_process_group() {
    // A shell's Ctrl-C delivers SIGINT (or a job-control kill delivers
    // whatever signal) to the *whole foreground process group* the
    // launched command sits in — never just that one pid. If the
    // detached workflow process shared the launcher's group, this would
    // kill it too, defeating the entire point of `--detach`. Simulated
    // here by putting the launcher in its own fresh group (exactly what
    // a shell's job control already does for a foreground command) and
    // signalling that whole group right after the launcher itself has
    // exited.
    use std::os::unix::process::CommandExt;

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
name: slow
nodes:
  - id: work
    kind: bash
    run: "until [ -f go.txt ]; do :; done; echo done > done.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let launcher = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml", "--detach"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let launcher_pgid = pid_of(&launcher); // `process_group(0)`: pgid == its own pid
    let output = launcher.wait_with_output().unwrap();
    assert!(output.status.success());
    let run_id = run_id_from(&output);

    // The launcher itself has already exited — this signals whatever
    // else is still in its group, if anything is.
    signal_group(launcher_pgid, Signal::SIGTERM).expect("the launcher's group is ours to signal");

    // The node blocks until `go.txt` appears, so it is still running when
    // the signal lands; release it and confirm the detached run survived
    // the signal and finished on its own.
    write(&repo.join("go.txt"), "go");
    let status = || stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    wait_until(
        || status().contains("finished"),
        || {
            format!(
                "the detached run must survive a signal to its launcher's group, got: {}",
                status()
            )
        },
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("done.txt"))
            .unwrap()
            .trim(),
        "done"
    );
}

/// The pid of a child this test spawned.
fn pid_of(child: &std::process::Child) -> Pid {
    Pid::try_from(child.id()).expect("a spawned child has a positive pid")
}

/// A pid a run wrote for the test to read back.
fn parse_pid(text: &str) -> Pid {
    text.trim()
        .parse::<u32>()
        .ok()
        .and_then(|raw| Pid::try_from(raw).ok())
        .unwrap_or_else(|| panic!("`{text}` is not a pid"))
}

/// §8.6 of the run contract hands the prior distribution — and the budget
/// warning derived from it — to the invocation that *creates* a run, and
/// `yunta resume` picks a run up instead of creating one. `--detach`
/// creates the run and gives it straight to such a child, so this is the
/// only invocation either can reach a person in, and a warning that asks
/// whether to spend is worth nothing once the child is already spending.
#[test]
fn run_detach_shows_the_distribution_and_the_budget_warning_before_handing_the_run_off() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let config = |limits: &str| {
        format!(
            "defaults:\n  isolation: none\n{limits}runners:\n  executor:\n    \
             - {{ adapter: claude-code, model: some-model }}\nadapters:\n  claude-code:\n    \
             binary: {binary}\n",
            binary = claude_code_stub().display()
        )
    };
    write(&repo.join(".yunta/config.yaml"), &config(""));
    write(
        &repo.join("wf.yaml"),
        "name: budgeted\nnodes:\n  - id: implement\n    kind: prompt\n    runner: executor\n    \
         prompt: \"Do the thing.\"\n",
    );
    // 500 tokens a run, so the p90 this history earns is a number a cap
    // can sit under.
    write(
        &repo.join(".claude-stub-lines.jsonl"),
        &format!(
            "{}\n{}\n",
            r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}"#,
            r#"{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":400,"output_tokens":100}}"#,
        ),
    );
    // Isolation `none` runs in this very checkout and refuses a dirty one.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "workflow and stub fixture"]);

    // The distribution stays silent below its own three-run floor, so
    // three runs are what earn this workflow one at all.
    for _ in 0..3 {
        let past = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
        assert!(past.status.success(), "stderr: {}", stderr(&past));
    }

    // A cap under what this workflow has historically spent, which is what
    // the warning is about. Each detached child then stops on that very
    // cap, exactly as the warning says it may.
    write(
        &repo.join(".yunta/config.yaml"),
        &config("limits:\n  max_tokens_per_run: 400\n"),
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "budget"]);

    // Every detached child is a process this test started: letting each
    // one reach its own stop keeps the next invocation off a moving log
    // and leaves nothing running once the temporary tree is gone.
    let settled = |run_id: &str| {
        let status = || stdout(&yunta_in!(&repo, &home, &["status", run_id]));
        wait_until(
            || {
                let text = status();
                ["finished", "failed", "waiting", "cancelled"]
                    .iter()
                    .any(|stop| text.contains(stop))
            },
            || format!("the detached run never reached a stop: {}", status()),
        );
    };

    let loud = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach"]);
    assert!(loud.status.success(), "stderr: {}", stderr(&loud));
    let text = stdout(&loud);
    let lines: Vec<&str> = text.lines().collect();
    // Three past runs, not four: the history is folded before this
    // invocation creates a run of its own, which is the only ordering in
    // which the warning beside it still precedes every token spent.
    let distribution = lines
        .iter()
        .position(|line| line.starts_with("3 past run(s) · "));
    let handoff = lines.iter().position(|line| line.contains("detached"));
    assert!(
        matches!((distribution, handoff), (Some(shown), Some(gone)) if shown < gone),
        "the distribution reaches the reader before the run is handed off: {text}"
    );
    assert!(
        stderr(&loud).contains("max_tokens_per_run") && stderr(&loud).contains("p90"),
        "the budget warning names the cap and what history says: {}",
        stderr(&loud)
    );
    // The CLI marks its own cautions, and marks each one once: the
    // sentence it prints states the fact and nothing about how it looks.
    let loud_stderr = stderr(&loud);
    let marked: Vec<&str> = loud_stderr
        .lines()
        .filter(|line| line.contains("max_tokens_per_run"))
        .collect();
    assert_eq!(marked.len(), 1, "the warning is printed once: {marked:?}");
    assert!(
        marked[0].starts_with("warning: ") && marked[0].matches("warning:").count() == 1,
        "one layer decides how a caution looks, so the prefix appears once: {}",
        marked[0]
    );
    settled(&run_id_from(&loud));

    let quiet = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach", "--quiet"]);
    assert!(quiet.status.success(), "stderr: {}", stderr(&quiet));
    assert_eq!(
        stdout(&quiet).lines().count(),
        1,
        "the distribution is context nobody asked for: {}",
        stdout(&quiet)
    );
    assert!(
        stderr(&quiet).contains("max_tokens_per_run") && stderr(&quiet).contains("p90"),
        "the warning asks for a decision before anything is spent, so it survives: {}",
        stderr(&quiet)
    );
    settled(&run_id_from(&quiet));

    let json = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach", "--json"]);
    assert!(json.status.success(), "stderr: {}", stderr(&json));
    let document: serde_json::Value = serde_json::from_slice(&json.stdout)
        .unwrap_or_else(|e| panic!("`--json` prints one document and nothing else: {e}"));
    assert_eq!(document["outcome"], "detached");
    assert!(
        stderr(&json).contains("max_tokens_per_run") && stderr(&json).contains("p90"),
        "the warning is not part of the document, and still reaches the reader: {}",
        stderr(&json)
    );
    let run_id = document["run_id"]
        .as_str()
        .expect("the document names the run");
    settled(run_id);
}

// --- `yunta resolve-gate` -------------------------------------------------

#[test]
fn yunta_resolve_gate_answers_an_exhausted_reroute_from_a_separate_process() {
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
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "touch fixed.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        stdout(&run).contains("paused"),
        "expected the exhausted re-route to pause the run, got: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    let run_id = run_id_from(&run);
    assert!(!repo.join("fixed.txt").exists());

    // A separate process, with no live surface attached to the run,
    // answers it.
    let resolve = yunta_in!(&repo, &home, &["resolve-gate", &run_id, "retry"]);
    assert!(
        resolve.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&resolve),
        String::from_utf8_lossy(&resolve.stderr)
    );
    assert!(stdout(&resolve).contains("resolved"));

    let status = || stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    wait_until(
        || status().contains("finished"),
        || {
            format!(
                "the run never reached finished after resolve-gate: {}",
                status()
            )
        },
    );
    assert!(repo.join("fixed.txt").exists());
}

#[test]
fn yunta_resolve_gate_rejects_an_unknown_option_without_touching_the_log() {
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
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "touch fixed.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let run_id = run_id_from(&run);

    let resolve = yunta_in!(&repo, &home, &["resolve-gate", &run_id, "nonexistent"]);
    assert!(!resolve.status.success());
    assert_eq!(
        String::from_utf8_lossy(&resolve.stderr).trim_end(),
        "error: option `nonexistent` isn't valid here — declared options: retry, abort",
        "the rejection lists exactly the options on this decision's menu"
    );

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("waiting"),
        "an invalid option must not touch the run's state: {}",
        stdout(&status)
    );
}

#[test]
fn resolving_a_run_that_is_not_parked_says_what_shows_where_it_is() {
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
name: fine
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "{}", stderr(&run));
    let run_id = run_id_from(&run);

    // A run that finished is parked on nothing, so the refusal describes
    // the state the run is in rather than the request — and a reader told
    // that is told what shows where the run actually stands.
    let resolve = yunta_in!(&repo, &home, &["resolve-gate", &run_id, "retry"]);
    assert!(!resolve.status.success());
    assert_eq!(
        stderr(&resolve).trim_end(),
        format!(
            "error: this run isn't parked at a pause — a live process may still be \
             driving it, or it already finished — `yunta status {run_id}` shows where it is"
        ),
        "the refusal names the state and what shows it"
    );
}

fn codex_stub() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../adapters/tests/fixtures/codex_stub.sh")
}

/// `--adapter <real>` makes every role resolve to its candidate on that
/// adapter, whatever `runners:` lists first — recorded as discards in
/// the log, never silently.
#[test]
fn adapter_flag_overrides_runner_resolution() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    let claude_args = root.path().join("claude-args.txt");
    let codex_args = root.path().join("codex-args.txt");

    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            r#"
runners:
  executor:
    - {{ adapter: codex, model: codex-model }}
    - {{ adapter: claude-code, model: claude-model }}
adapters:
  codex:
    binary: {codex}
  claude-code:
    binary: {claude}
secrets: [CLAUDE_STUB_ARGS_FILE, CODEX_STUB_ARGS_FILE]
"#,
            codex = codex_stub().display(),
            claude = claude_code_stub().display()
        ),
    );
    write(
        &repo.join("wf.yaml"),
        "name: override\nnodes:\n  - id: implement\n    kind: prompt\n    runner: executor\n    prompt: \"Do the thing.\"\n",
    );
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

    let run = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml", "--adapter", "claude-code"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .env("CLAUDE_STUB_ARGS_FILE", &claude_args)
        .env("CODEX_STUB_ARGS_FILE", &codex_args)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"), "got: {}", stdout(&run));
    // Both stubs record every invocation, the health probe (`--version`)
    // included: a session is the invocation that is not the probe.
    let ran_a_session =
        |args: &Path| std::fs::read_to_string(args).is_ok_and(|text| !text.contains("--version"));
    assert!(
        ran_a_session(&claude_args),
        "the override adapter must have run the session"
    );
    assert!(
        !ran_a_session(&codex_args),
        "the first-listed candidate must not have run a session"
    );
}

/// `--adapter mock --fixture <path>` runs a workflow against a scripted
/// fixture with no test case and no real CLI on the machine.
#[test]
fn adapter_mock_with_fixture_runs() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "runners:\n  executor:\n    - { adapter: claude-code, model: claude-model }\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: mocked\nnodes:\n  - id: implement\n    kind: prompt\n    runner: executor\n    prompt: \"Do the thing.\"\n    artifacts:\n      produces: [note.md]\n",
    );
    write(
        &repo.join("fixture.yaml"),
        "sessions:\n  - effects:\n      - { path: \"{{run.dir}}/artifacts/note.md\", content: \"done\\n\" }\n    outcome: { type: completed, summary: \"noted\" }\n",
    );

    let refused = yunta_in!(&repo, &home, &["run", "wf.yaml", "--adapter", "mock"]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--fixture"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    let run = yunta_in!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
        ]
    );
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"), "got: {}", stdout(&run));
}

/// A `~` in `storage.path` lands under the home the process was given —
/// never a directory literally named `~` beside the repository.
#[test]
fn tilde_in_storage_path_resolves_under_home() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    write(
        &repo.join(".yunta/config.yaml"),
        "storage: { path: ~/state/yunta.db }\npaths: { runs: ~/state/runs, worktrees: ~/state/worktrees }\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: tilde\nnodes:\n  - id: touch\n    kind: bash\n    run: \"true\"\n",
    );
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("HOME", &home)
        .env_remove("YUNTA_HOME")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        home.join("state/yunta.db").exists(),
        "the event log lives under the home"
    );
    assert!(
        !repo.join("~").exists(),
        "no literal `~` directory beside the repository"
    );
}

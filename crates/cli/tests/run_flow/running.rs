//! A run from the command line, in the words and files it leaves.

use super::*;

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
    assert_eq!(state["summary"], "2 of 2 nodes done · finished");
    assert_eq!(node_of(&state, "touch"), finished_node("touch"));
    assert_eq!(node_of(&state, "verify"), finished_node("verify"));

    // Resuming a finished run is a clean no-op.
    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
    assert!(
        resume.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resume.stderr)
    );
    assert!(stdout(&resume).contains("finished"));
}

#[cfg(unix)]
#[test]
fn the_run_files_are_private_to_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join("wf.yaml"),
        "name: private\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    let run_dir = home.join("runs").join(run_id_from(&run));
    for file in ["events.jsonl", "manifest.yaml"] {
        let mode = std::fs::metadata(run_dir.join(file))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{file} is readable by others");
    }
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
    // in, since a real run's session env holds only what the engine sets for it).
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

#[test]
fn a_gate_with_stdin_not_a_tty_pauses_instead_of_hanging() {
    // A `yunta run` whose stdin isn't a terminal (exactly `cargo test`'s
    // own usual case, made explicit here with `Stdio::null()` so this
    // doesn't depend on whatever stdin the test binary inherited) must
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

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    command.args(["run", "wf.yaml"]);
    let output = CliChild::spawn(command, None)
        .expect("failed to spawn the yunta binary")
        .wait_with_output()
        .expect("the yunta binary did not finish within the test deadline");

    assert!(!output.status.success());
    let text = stdout(&output);
    let run_id = run_id_from(&output);
    let called = yunta_testkit::handle(&run_id);
    assert!(
        text.lines()
            .any(|l| l.contains("· needs you ·") && l.trim_end().ends_with(called)),
        "a gate with no TTY degrades to a paused run instead of hanging: {text}"
    );
    assert!(
        ["retry", "abort"]
            .iter()
            .all(|option| text.contains(&format!("yunta resolve-gate {called} {option}"))),
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
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    command.args(["run", "wf.yaml"]);
    let mut run = CliChild::spawn_with_stderr_log(command, Some(&progress_log)).unwrap();

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
        stdout(&output).contains("interrupted by a person"),
        "the closing block names the outcome: {}",
        stdout(&output)
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
    let on_its_branch = format!("yunta/run/{child_id}:child.txt");
    assert_eq!(
        git_output(&repo, &["show", &on_its_branch]).trim(),
        "from-child"
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
    let verify = node_of(&state, "verify");
    assert_eq!(verify["state"], "failed", "{state:#}");
    assert_eq!(
        verify["detail"], "exit 1",
        "a command that said nothing is quoted with nothing promised: {state:#}"
    );
    assert_eq!(verify["command_exit"]["code"], 1, "{state:#}");
    assert!(
        verify["command_exit"].get("tail").is_none(),
        "a command that printed nothing has no last lines to quote: {state:#}"
    );
}

/// The entry `nodes` carries for `id` — a list now, in the order the
/// workflow declares its nodes.
fn node_of(document: &serde_json::Value, id: &str) -> serde_json::Value {
    document["nodes"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|node| node["id"] == id))
        .cloned()
        .unwrap_or_else(|| panic!("`{id}` is one of the run's nodes: {document:#}"))
}

/// The entry a bash node that exited cleanly renders as.
fn finished_node(id: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "state": "finished", "detail": "exit 0"})
}

//! Stopping a live run: from its own terminal, from another process, and after a crash.

use super::*;

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

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    command.args(["run", "wf.yaml"]);
    let yunta = CliChild::spawn(command, None).unwrap();

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
    // The child ignores SIGINT on purpose: if anything below fails before
    // the engine kills it, this is what keeps it from outliving the test.
    let mut child_group = ProcessGroupCleanup::new(parse_pid(&child_pid));

    // Simulated Ctrl-C: SIGINT to the yunta process.
    signal_process(pid_of(&yunta), Signal::SIGINT).expect("yunta is alive to be interrupted");

    let output = yunta.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("interrupted by a person"),
        "the pause must say why: {text}"
    );

    // Zero zombies: the SIGINT-ignoring child is dead anyway.
    assert_eq!(
        liveness(parse_pid(&child_pid)),
        Liveness::Dead,
        "the stubborn child must be dead"
    );
    child_group.disarm();

    // The `none` lock is released, and engine.json is gone.
    assert!(!repo.join(".git/yunta-none.lock").exists());
    let run_id = run_id_from(&output);
    assert!(!home
        .join("runs")
        .join(&run_id)
        .join("scratch/engine.json")
        .exists());
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
    let child_pid = std::fs::read_to_string(repo.join("child.pid"))
        .unwrap()
        .trim()
        .to_string();
    // If anything below fails before `cancel` kills the child, this is
    // what keeps it from outliving the test.
    let mut child_group = ProcessGroupCleanup::new(parse_pid(&child_pid));

    let cancel = yunta_in!(&repo, &home, &["cancel", &run_id]);
    assert!(
        cancel.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&cancel),
        String::from_utf8_lossy(&cancel.stderr)
    );
    assert!(
        stdout(&cancel).contains(" · stopped"),
        "got: {}",
        stdout(&cancel)
    );

    // The run process exits, its child is dead, the log is terminal.
    let output = yunta.wait_with_output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("interrupted by a person"));
    assert_eq!(
        liveness(parse_pid(&child_pid)),
        Liveness::Dead,
        "the blocked child must be dead"
    );
    child_group.disarm();

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("interrupted by a person"),
        "got: {}",
        stdout(&status)
    );
}

/// A run that paused once and was woken again is running, whatever its
/// log said before the wake: `cancel` reaches the engine that woke it.
#[test]
fn yunta_cancel_stops_a_run_woken_after_a_pause() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    let run_id = woken_long_run(&repo, &home);
    let child_pid = parse_pid(&std::fs::read_to_string(repo.join("child.pid")).unwrap());
    let mut child_group = ProcessGroupCleanup::new(child_pid);

    let cancel = yunta_in!(&repo, &home, &["cancel", &run_id]);

    assert!(
        cancel.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    assert!(
        stdout(&cancel).contains("signalling the live engine"),
        "got: {}",
        stdout(&cancel)
    );
    wait_until(
        || liveness(child_pid) == Liveness::Dead,
        || "the woken run's child outlived the cancel".into(),
    );
    child_group.disarm();
}

/// A run in `repo` that parked on a gate nobody answered, then was woken
/// by its answer and is running a node that never ends on its own — its
/// pid in `child.pid`. Answers the run's id.
fn woken_long_run(repo: &Path, home: &Path) -> String {
    std::fs::create_dir_all(repo).unwrap();
    init_repo(repo);
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: woken
nodes:
  - { id: approve, kind: gate, assignee: lead }
  - id: long
    kind: bash
    depends_on: [approve]
    run: "echo $$ > child.pid; tail -f /dev/null"
"#,
    );
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "fixtures"]);

    // No terminal answers the gate, so the run parks on it.
    let parked = yunta_in!(repo, home, &["run", "wf.yaml"]);
    assert!(stdout(&parked).contains("approve"), "{}", stdout(&parked));
    let run_id = only_run_id(home);
    let resolved = yunta_in!(repo, home, &["resolve-gate", &run_id, "approve"]);
    assert!(
        resolved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    wait_until(
        || marker_written(&repo.join("child.pid")),
        || "the woken run never started `long`".into(),
    );
    run_id
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
    let child_pid = parse_pid(&std::fs::read_to_string(repo.join("child.pid")).unwrap());
    let mut child_group = ProcessGroupCleanup::new(child_pid);

    // Simulated crash: SIGKILL gives the engine no chance to clean up —
    // engine.json survives with the orphaned process group in it.
    signal_process(pid_of(&yunta), Signal::SIGKILL).expect("yunta is alive to be killed");
    yunta.wait_until_exit().expect("the killed engine exits");
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
        stdout(&cancel).contains("had already died"),
        "got: {}",
        stdout(&cancel)
    );
    yunta
        .wait_with_output()
        .expect("the cancelled group is closed and the engine is reaped");

    // The orphaned child is dead, the registry is gone, the log records
    // the crash-cancellation. `kill -0` succeeds on a zombie (the
    // SIGKILLed engine never reaped it), so check the process state:
    // gone or Z both mean the kill landed. `cancel` returns once the kill
    // is sent; the child leaves the process table on the kernel's
    // schedule, not ours.
    let dead = |state: &str| state.is_empty() || state.starts_with('Z');
    wait_until(
        || dead(&process_state(&child_pid.as_i32().to_string())),
        || {
            format!(
                "the orphaned child must be dead, ps state: {}",
                process_state(&child_pid.as_i32().to_string())
            )
        },
    );
    child_group.disarm();
    assert!(!engine_json.exists());
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("its engine had died, and a cancel recorded the stop"),
        "got: {}",
        stdout(&status)
    );
}

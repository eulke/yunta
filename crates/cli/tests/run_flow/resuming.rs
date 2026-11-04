//! A paused run taken up again, with what it froze when it was born.

use super::*;

#[test]
fn a_short_declared_secret_refuses_a_resume_before_the_wake_writes_anything() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    // Declared but unset when the run is born and when it first wakes, so
    // nothing refuses it then.
    write(
        &repo.join(".yunta/config.yaml"),
        "secrets: [YUNTA_TEST_SHORT_TOKEN]\n",
    );
    write(&repo.join("wf.yaml"), PAUSES_ON_A_FAILING_NODE);

    let paused = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        stdout(&paused).contains("needs you"),
        "got: {}",
        stdout(&paused)
    );
    let run_id = run_id_from(&paused);
    let events = home.join("runs").join(&run_id).join("events.jsonl");

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    let resume = command
        .args(["resume", run_id.as_str()])
        .env("YUNTA_TEST_SHORT_TOKEN", "abc")
        .output()
        .unwrap();
    assert!(
        !resume.status.success(),
        "a 3-character secret must refuse the resume"
    );
    assert!(
        stderr(&resume).contains("YUNTA_TEST_SHORT_TOKEN"),
        "the refusal names the variable: {}",
        stderr(&resume)
    );
    let log = std::fs::read_to_string(&events).unwrap();
    assert!(
        !log.contains("run_resumed"),
        "the refused resume wrote into the log"
    );
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
    assert!(stdout(&run).contains("needs you"), "got: {}", stdout(&run));
    let run_id = run_id_from(&run);

    let resume = yunta_in!(&repo, &home, &["resume", &run_id]);
    assert!(
        stdout(&resume).contains("needs you"),
        "got: {}",
        stdout(&resume)
    );

    // Resume must operate on the very checkout `run` worked in — never a
    // second one, and never the original checkout.
    assert!(!repo.join("attempt-marker.txt").exists());
    let checkouts = yunta_testkit::pool_checkouts(&home);
    assert_eq!(
        checkouts.len(),
        1,
        "resume must reuse the run's own checkout, not take another"
    );
    assert!(checkouts[0].join("marker.txt").exists());
    assert!(checkouts[0].join("attempt-marker.txt").exists());
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
    run: "echo $$ > child.pid; echo x > started.txt; test -f go.txt || tail -f /dev/null"
"#,
    );

    // Crash the engine mid-node (the node blocks until go.txt exists).
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    command.args(["run", "wf.yaml"]);
    let mut yunta = CliChild::spawn(command, None).unwrap();
    let worktree = wait_for(
        || {
            yunta_testkit::pool_checkouts(&home)
                .into_iter()
                .find(|checkout| checkout.join("started.txt").exists())
        },
        || "the node never started".into(),
    );
    let run_id = only_run_id(&home);
    signal_process(pid_of(&yunta), Signal::SIGKILL).expect("yunta is alive to be killed");
    yunta.wait_until_exit().expect("the crashed engine exits");

    // The condition the restarted node needs, in the ORIGINAL worktree —
    // then move the config's worktrees root somewhere else entirely.
    let orphaned_group = parse_pid(&std::fs::read_to_string(worktree.join("child.pid")).unwrap());
    let mut child_group = ProcessGroupCleanup::new(orphaned_group);
    child_group
        .close()
        .expect("the previous node's group is killed before resume");
    write(&worktree.join("go.txt"), "go");
    yunta
        .wait_with_output()
        .expect("the crashed engine is reaped after its child group is closed");
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
        String::from_utf8_lossy(&output.stdout).contains("interrupted by a person"),
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

//! What `list`, `status`, `doctor` and `verify` say of what is there.

use super::*;

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
        "  greet: Says hello\n    inputs       --input greeting=… (string, optional): What to say\n    run          yunta run greet",
        "list shows the workflow, its description, its one optional input and how to run it"
    );

    // The same catalog under the ASCII look: the input's placeholder is three
    // dots, and no Unicode character is written beside the input.
    let ascii = [("YUNTA_GLYPHS".to_string(), "ascii".to_string())];
    let ascii_list = yunta_in!(&repo, &home, &["list"], &ascii);
    assert!(
        ascii_list.status.success(),
        "stderr: {}",
        stderr(&ascii_list)
    );
    assert_eq!(
        stdout(&ascii_list).trim_end(),
        "  greet: Says hello\n    inputs       --input greeting=... (string, optional): What to say\n    run          yunta run greet",
        "under the ASCII look the input's placeholder is three dots, not an ellipsis character"
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

    let list = yunta_in!(&repo, &home, &["list", "--runs", "--full"]);
    assert!(list.status.success());
    let text = stdout(&list);
    let mut lines = text
        .lines()
        .skip_while(|line| *line != "closed, newest first");
    assert_eq!(
        lines.next(),
        Some("closed, newest first"),
        "a finished run is listed under what can be done about it: {text}"
    );
    let row = lines.next().unwrap_or_default();
    assert!(
        row.contains(&format!(
            "✓ {}  only-node (default)",
            yunta_testkit::handle(&run_id)
        )),
        "the row says how the run closed and names the workflow and the mode: {text}"
    );
    assert!(
        lines.next().is_none_or(str::is_empty),
        "a run that finished has nothing under it to act on: {text}"
    );
}

#[test]
fn doctor_cautions_when_no_runner_is_declared_and_no_workflow_needs_one() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let doctor = yunta_in!(&repo, &home, &["doctor"]);
    assert!(doctor.status.success(), "{}", stderr(&doctor));
    assert!(
        yunta_testkit::checked(&stdout(&doctor), "runners")
            .is_some_and(|said| said.starts_with("none declared")),
        "{}",
        stdout(&doctor)
    );
    assert!(
        stdout(&doctor).contains("declare a runner in .yunta/config.yaml"),
        "{}",
        stdout(&doctor)
    );
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
        state["summary"], "2 of 2 nodes done · finished",
        "the normative counters read both nodes done and finished, and a run with no \
         reroute says nothing about them"
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
        state["summary"], "3 of 3 nodes done \u{b7} 1 skipped by `fan-only` \u{b7} finished",
        "the group and both its children are inside the denominator, and the \
         only skipped node is the top-level one the mode leaves out"
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

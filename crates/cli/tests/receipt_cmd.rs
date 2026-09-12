//! `yunta receipt <run_id>` end to end: the real compiled
//! binary, same style `stats_cmd.rs`/`run_flow.rs` use — this is what
//! proves the CLI wiring (writing `receipt.md`/`receipt.json` to
//! `run.dir`, the `--json` flag, refusing a non-terminal run) on top of
//! `yunta_engine::receipt`'s own unit-tested derivation.

use yunta_testkit::{init_repo, run_id_from, stderr, stdout, write, yunta_in};

fn bash_only_workflow() -> &'static str {
    r#"
name: bash-only-receipt
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
  - id: verify
    kind: bash
    depends_on: [touch]
    run: "test -f made.txt"
"#
}

#[test]
fn receipt_writes_both_formats_to_run_dir_and_prints_markdown_by_default() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/bash-only-receipt.yaml"),
        bash_only_workflow(),
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"]
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id]);
    assert!(receipt_out.status.success(), "{}", stderr(&receipt_out));
    let markdown = stdout(&receipt_out);
    assert!(markdown.starts_with(&format!("# Verified Work Receipt — run {run_id}")));
    assert!(
        markdown
            .lines()
            .any(|l| l == "- baseline: not used by this workflow"),
        "{markdown}"
    );
    assert!(
        markdown.lines().any(|l| l.starts_with("- ✓ event chain: ")),
        "{markdown}"
    );

    // A bash-only workflow has no tasks at all — 0/0 criteria, the
    // honest reading (marked ✗, not a green ✓), never a manufactured pass.
    assert!(
        markdown
            .lines()
            .any(|l| l == "- ✗ 0/0 criteria green (commands + exit codes below)"),
        "{markdown}"
    );

    let run_dir = home.join("runs").join(&run_id);
    let written_md = std::fs::read_to_string(run_dir.join("receipt.md")).unwrap();
    assert_eq!(written_md, markdown);
    let written_json = std::fs::read_to_string(run_dir.join("receipt.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&written_json).unwrap();
    assert_eq!(parsed["run_id"], run_id);
    assert_eq!(parsed["terminal_state"], "done");
}

#[test]
fn receipt_json_flag_prints_the_json_that_was_written() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/bash-only-receipt.yaml"),
        bash_only_workflow(),
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"]
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id, "--json"]);
    assert!(receipt_out.status.success(), "{}", stderr(&receipt_out));
    let printed: serde_json::Value = serde_json::from_str(&stdout(&receipt_out)).unwrap();

    let run_dir = home.join("runs").join(&run_id);
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join("receipt.json")).unwrap())
            .unwrap();
    assert_eq!(printed, written);
}

#[test]
fn receipt_refuses_a_run_that_has_not_finished() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // No `on_failure` at all — a failing node just pauses the run.
    write(
        &repo.join(".yunta/workflows/never-finishes.yaml"),
        "name: never-finishes\nnodes:\n  - id: broken\n    kind: bash\n    run: \"false\"\n",
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/never-finishes.yaml"]
    );
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id]);
    assert!(!receipt_out.status.success());
    assert_eq!(
        stderr(&receipt_out).trim_end(),
        format!(
            "error: run `{run_id}` hasn't reached a terminal state yet — \
             `yunta status {run_id}` shows where it is; a receipt is only generated \
             once a run finishes"
        ),
        "the refusal explains the run is not terminal and points at `yunta status`"
    );

    let run_dir = home.join("runs").join(&run_id);
    assert!(!run_dir.join("receipt.md").exists());
    assert!(!run_dir.join("receipt.json").exists());
}

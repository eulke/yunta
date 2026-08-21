//! `yunta receipt <run_id>` end to end (D54, T10.4): the real compiled
//! binary, same style `stats_cmd.rs`/`run_flow.rs` use — this is what
//! proves the CLI wiring (writing `receipt.md`/`receipt.json` to
//! `run.dir`, the `--json` flag, refusing a non-terminal run) on top of
//! `yunta_engine::receipt`'s own unit-tested derivation.

use std::path::Path;
use std::process::Output;

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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
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

    let run_out = yunta_in(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"],
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in(&repo, &home, &["receipt", &run_id]);
    assert!(receipt_out.status.success(), "{}", stderr(&receipt_out));
    let markdown = stdout(&receipt_out);
    assert!(markdown.starts_with(&format!("# Verified Work Receipt — run {run_id}")));
    assert!(markdown.contains("baseline: not used by this workflow"));
    assert!(markdown.contains("event chain:"));

    // A bash-only workflow has no ledger tasks at all — 0/0 criteria,
    // the honest reading, never a manufactured pass.
    assert!(markdown.contains("0/0 criteria green"), "{markdown}");

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

    let run_out = yunta_in(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"],
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in(&repo, &home, &["receipt", &run_id, "--json"]);
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

    let run_out = yunta_in(
        &repo,
        &home,
        &["run", ".yunta/workflows/never-finishes.yaml"],
    );
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in(&repo, &home, &["receipt", &run_id]);
    assert!(!receipt_out.status.success());
    let err = stderr(&receipt_out);
    assert!(err.contains("hasn't reached a terminal state"), "{err}");
    assert!(err.contains("yunta status"), "{err}");

    let run_dir = home.join("runs").join(&run_id);
    assert!(!run_dir.join("receipt.md").exists());
    assert!(!run_dir.join("receipt.json").exists());
}

//! End-to-end: a node whose declared re-route never fires
//! across enough historical runs shows up both in `yunta check` and in
//! `yunta stats --workflow`, never as a reason either command fails.

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

const FLAKY_LINT_WORKFLOW: &str = r#"
name: flaky-lint
nodes:
  - id: lint
    kind: bash
    run: "true"
    on_failure: { goto: fix, max_reroutes: 2 }
  - id: fix
    kind: bash
    run: "true"
"#;

#[test]
fn a_never_triggered_reroute_surfaces_in_both_check_and_stats() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join("wf.yaml"), FLAKY_LINT_WORKFLOW);

    for _ in 0..3 {
        let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
        assert!(run.status.success(), "stderr: {}", stderr(&run));
    }

    let check = yunta_in(&repo, &home, &["check", "wf.yaml"]);
    assert!(
        check.status.success(),
        "advisory findings must never fail check"
    );
    assert!(
        stderr(&check).contains("never fired"),
        "got: {}",
        stderr(&check)
    );
    assert!(
        stdout(&check).contains("OK"),
        "check's own OK line must still print, got: {}",
        stdout(&check)
    );

    let stats = yunta_in(&repo, &home, &["stats", "--workflow", "flaky-lint"]);
    assert!(stats.status.success());
    assert!(
        stdout(&stats).contains("never fired"),
        "got: {}",
        stdout(&stats)
    );

    let stats_json = yunta_in(
        &repo,
        &home,
        &["stats", "--workflow", "flaky-lint", "--json"],
    );
    assert!(stats_json.status.success());
    assert!(
        stdout(&stats_json).contains("never_triggered_reroutes"),
        "got: {}",
        stdout(&stats_json)
    );
}

#[test]
fn fewer_than_three_runs_surfaces_no_findings_at_all() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join("wf.yaml"), FLAKY_LINT_WORKFLOW);

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());

    let check = yunta_in(&repo, &home, &["check", "wf.yaml"]);
    assert!(check.status.success());
    assert!(
        !stderr(&check).contains("verification performance"),
        "got: {}",
        stderr(&check)
    );
}

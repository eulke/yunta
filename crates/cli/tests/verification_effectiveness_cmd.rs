//! End-to-end: a node whose declared re-route never fires
//! across enough historical runs shows up both in `yunta check` and in
//! `yunta stats --workflow`, never as a reason either command fails.

use yunta_testkit::{init_repo, stderr, stdout, write, yunta_in};

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
        let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
        assert!(run.status.success(), "stderr: {}", stderr(&run));
    }

    let check = yunta_in!(&repo, &home, &["check", "wf.yaml"]);
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

    let stats = yunta_in!(&repo, &home, &["stats", "--workflow", "flaky-lint"]);
    assert!(stats.status.success());
    assert!(
        stdout(&stats).contains("never fired"),
        "got: {}",
        stdout(&stats)
    );

    let stats_json = yunta_in!(
        &repo,
        &home,
        &["stats", "--workflow", "flaky-lint", "--json"]
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

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success());

    let check = yunta_in!(&repo, &home, &["check", "wf.yaml"]);
    assert!(check.status.success());
    assert!(
        !stderr(&check).contains("verification performance"),
        "got: {}",
        stderr(&check)
    );
}

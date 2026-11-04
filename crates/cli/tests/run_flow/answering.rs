//! `yunta resolve-gate`: answering a parked run from another process.

use super::*;

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
        stdout(&run).contains("needs you"),
        "expected the exhausted reroute to pause the run, got: {}\nstderr: {}",
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
        unwrapped(&String::from_utf8_lossy(&resolve.stderr)),
        "error: option `nonexistent` isn't valid here; declared options: retry, abort",
        "the rejection lists exactly the options on this decision's menu"
    );

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        stdout(&status).contains("needs you"),
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
        unwrapped(&stderr(&resolve)),
        format!(
            "error: this run isn't parked at a pause: a live process may still be \
             driving it, or it already finished; `yunta status {}` shows where it is",
            yunta_testkit::handle(&run_id)
        ),
        "the refusal names the state and what shows it"
    );
}

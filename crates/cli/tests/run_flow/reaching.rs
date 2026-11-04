//! How a run is reached: the adapter a flag names, a path in the config, and the one recipe `run` and `test` share.

use super::*;

#[test]
fn yunta_test_and_yunta_run_execute_the_same_recipe() {
    // One execution path: a case resolves its workflow through the same
    // catalog, refuses on the same static check, freezes the same
    // manifest and drives through the same call. So a workflow this
    // binary would refuse to run is refused to a case in the same
    // words, and one it runs reaches the same stop.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/recipe.yaml"),
        "name: recipe\nnodes:\n  - id: touch\n    kind: bash\n    run: \"echo made > made.txt\"\n",
    );
    write(
        &repo.join(".yunta/tests/fixtures/none.yaml"),
        "dialect: claude-code\nsessions: []\n",
    );
    write(
        &repo.join(".yunta/tests/recipe.yaml"),
        "workflow: recipe\nfixture: fixtures/none.yaml\nexpect:\n  final_state: finished\n  nodes:\n    touch: finished\n",
    );

    let driven = yunta_in!(
        &repo,
        &home,
        &[
            "run",
            "recipe",
            "--adapter",
            "mock",
            "--fixture",
            ".yunta/tests/fixtures/none.yaml",
        ]
    );
    assert!(driven.status.success(), "{}", stderr(&driven));
    assert!(stdout(&driven).contains("finished"), "{}", stdout(&driven));

    let cased = yunta_in!(&repo, &home, &["test"]);
    assert!(cased.status.success(), "{}", stdout(&cased));
    assert!(
        yunta_testkit::checked(&stdout(&cased), "recipe").as_deref() == Some("ok"),
        "{}",
        stdout(&cased)
    );

    // Now the same workflow, broken the same way for both: a node
    // rerouting to one the only mode leaves out. `check` refuses it,
    // and a case is refused by that same check rather than running a
    // workflow `yunta run` would not.
    write(
        &repo.join(".yunta/workflows/recipe.yaml"),
        "name: recipe\nmodes:\n  only: { include: [touch] }\nnodes:\n  - id: touch\n    kind: bash\n    run: \"true\"\n    on_failure: { goto: mend, max_reroutes: 1 }\n  - id: mend\n    kind: bash\n    run: \"true\"\n",
    );

    let refused_run = yunta_in!(
        &repo,
        &home,
        &[
            "run",
            "recipe",
            "--adapter",
            "mock",
            "--fixture",
            ".yunta/tests/fixtures/none.yaml",
        ]
    );
    assert!(!refused_run.status.success());
    let said = unwrapped(&String::from_utf8_lossy(&refused_run.stderr));

    let refused_case = yunta_in!(&repo, &home, &["test"]);
    assert!(!refused_case.status.success());
    let complaint = "a mode that keeps a node keeps what it reroutes to";
    assert!(said.contains(complaint), "yunta run: {said}");
    assert!(
        unwrapped(&stdout(&refused_case)).contains(complaint),
        "yunta test: {}",
        stdout(&refused_case)
    );
}

/// `--adapter <real>` makes every role resolve to its candidate on that
/// adapter, whatever `runners:` lists first — recorded as discards in
/// the log, never silently.
#[test]
fn adapter_flag_overrides_runner_resolution() {
    let repo = Override::new();
    let run = repo.yunta(&["run", "wf.yaml", "--adapter", "claude-code"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout(&run).contains("finished"), "got: {}", stdout(&run));
    assert_eq!(
        repo.ran_on_claude_only(),
        (true, false),
        "the override adapter runs the session, the first-listed candidate none"
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
        "dialect: claude-code\nsessions:\n  - effects:\n      - { path: \"{{run.staging}}/implement/note.md\", content: \"done\\n\" }\n    outcome: { type: completed, summary: \"noted\" }\n",
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
    let checkout = Checkout::under(root.path())
        .without_yunta_home()
        .config(
            "storage: { path: ~/state/yunta.db }\npaths: { runs: ~/state/runs, worktrees: ~/state/worktrees }\n",
        )
        .workflow(
            "wf",
            "name: tilde\nnodes:\n  - id: touch\n    kind: bash\n    run: \"true\"\n",
        );

    let run = yunta_at!(checkout, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        stderr(&run)
    );
    assert!(
        checkout.home.join("state/yunta.db").exists(),
        "the event log lives under the home"
    );
    assert!(
        !checkout.repo.join("~").exists(),
        "no literal `~` directory beside the repository"
    );
}

/// A detached run is driven by a `yunta resume` of its own, which holds
/// the adapter the run was started on rather than `runners:`'s first.
#[test]
fn a_detached_run_keeps_the_adapter_it_was_started_on() {
    let repo = Override::new();
    let detached = repo.yunta(&["run", "wf.yaml", "--detach", "--adapter", "claude-code"]);
    assert!(detached.status.success(), "stderr: {}", stderr(&detached));
    wait_until(
        || repo.ran_on_claude_only() != (false, false),
        || "the detached run never opened its session".into(),
    );
    assert_eq!(
        repo.ran_on_claude_only(),
        (true, false),
        "the detached child runs the session on the adapter it was started on"
    );
    let run_id = run_id_from(&detached);
    wait_until(
        || {
            let status = repo.yunta(&["status", &run_id, "--json"]);
            serde_json::from_slice::<serde_json::Value>(&status.stdout).is_ok_and(|state| {
                state["summary"]
                    .as_str()
                    .is_some_and(|summary| summary.ends_with("finished"))
            })
        },
        || "the detached run never finished".into(),
    );
}

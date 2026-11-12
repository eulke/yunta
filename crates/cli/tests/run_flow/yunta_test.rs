//! `yunta test`: a case runs what `yunta run` runs, and says what it expected.

use super::*;

use yunta_testkit::workflows::PROMOTABLE;

#[test]
fn yunta_test_runs_a_case_with_the_mock_adapter_and_checks_expectations() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        r#"
runners:
  planner:
    - { adapter: claude-code, model: real-model }
  executor:
    - { adapter: claude-code, model: real-model }
"#,
    );
    write(
        &repo.join(".yunta/workflows/mini-bootstrap.yaml"),
        r#"
name: mini-bootstrap
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#,
    );
    // The scripted planner hands its tasks document over the way a real one
    // does: a tool call the engine answers, and a file the engine writes.
    write(
        &repo.join(".yunta/tests/fixtures/happy.yaml"),
        r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: "Make it"
                scope: ["made.txt"]
                criteria:
                  - cmd: "test -f made.txt"
    outcome: { type: completed, summary: "planned" }
  - effects:
      - { path: made.txt, content: "made" }
    outcome: { type: completed, summary: "made it" }
"#,
    );
    write(
        &repo.join(".yunta/tests/happy-path.yaml"),
        r#"
workflow: mini-bootstrap
fixture: fixtures/happy.yaml
expect:
  final_state: finished
  nodes:
    plan: finished
    implement: finished
  tasks:
    T001: done
"#,
    );

    let output = yunta_in!(&repo, &home, &["test"]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "stdout: {text}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        (
            yunta_testkit::checked(&text, "happy-path").as_deref(),
            text.lines()
                .last()
                .is_some_and(|line| line.ends_with("yunta test · 1 case, 0 failed"))
        ),
        (Some("ok"), true),
        "the one case runs against the mock adapter and every expectation holds: {text}"
    );
}

/// A node the case's mode leaves out is `skipped`, the word `status`
/// prints for it, and a case can say so.
#[test]
fn a_case_expecting_a_node_its_mode_leaves_out_to_be_skipped_passes() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/workflows/narrow.yaml"),
        "name: narrow\nmodes:\n  short: { include: [first] }\n  long: { include: all }\nnodes:\n  \
         - { id: first, kind: bash, run: \"true\" }\n  \
         - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n",
    );
    write(
        &repo.join(".yunta/tests/fixtures/none.yaml"),
        "dialect: claude-code\nsessions: []\n",
    );
    write(
        &repo.join(".yunta/tests/short-skips-second.yaml"),
        "workflow: narrow\nmode: short\nfixture: fixtures/none.yaml\nexpect:\n  \
         final_state: finished\n  nodes:\n    first: finished\n    second: skipped\n",
    );

    let output = yunta_in!(&repo, &home, &["test"]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "stdout: {text}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        yunta_testkit::checked(&text, "short-skips-second").as_deref(),
        Some("ok"),
        "{text}"
    );
}

#[test]
fn a_document_input_gives_a_loop_its_tasks_with_no_node_producing_them() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        r#"
runners:
  executor:
    - { adapter: claude-code, model: real-model }
"#,
    );
    // Nothing here produces a tasks document: the run is born holding
    // the one its `tasks` input named.
    write(
        &repo.join(".yunta/workflows/run-a-plan.yaml"),
        r#"
name: run-a-plan
inputs:
  plan:
    type: document
    kind: tasks
    required: true
nodes:
  - id: implement
    kind: loop
    runner: executor
    until: all_tasks_complete
    prompt: "Implement your task."
"#,
    );
    write(
        &repo.join(".yunta/tests/seed/plan/tasks.yaml"),
        r#"
tasks:
  - id: T001
    title: "Make it"
    scope: ["made.txt"]
    criteria:
      - cmd: "test -f made.txt"
"#,
    );
    write(
        &repo.join(".yunta/tests/fixtures/one-task.yaml"),
        r#"
dialect: claude-code
sessions:
  - effects:
      - { path: made.txt, content: "made" }
    outcome: { type: completed, summary: "made it" }
"#,
    );
    write(
        &repo.join(".yunta/tests/from-a-document.yaml"),
        r#"
workflow: run-a-plan
inputs: { plan: plan/tasks.yaml }
worktree: seed
fixture: fixtures/one-task.yaml
expect:
  final_state: finished
  nodes:
    implement: finished
  tasks:
    T001: done
"#,
    );

    let output = yunta_in!(&repo, &home, &["test"]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "stdout: {text}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        (
            yunta_testkit::checked(&text, "from-a-document").as_deref(),
            text.lines()
                .last()
                .is_some_and(|line| line.ends_with("yunta test · 1 case, 0 failed"))
        ),
        (Some("ok"), true),
        "the loop finds the tasks the input brought in: {text}"
    );
}

#[test]
fn a_failing_expectation_fails_yunta_test_naming_the_mismatch() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/one-bash.yaml"),
        r#"
name: one-bash
nodes:
  - id: fails
    kind: bash
    run: "exit 1"
"#,
    );
    write(
        &repo.join(".yunta/tests/fixtures/empty.yaml"),
        "dialect: claude-code\nsessions: []\n",
    );
    write(
        &repo.join(".yunta/tests/wrong-expect.yaml"),
        r#"
workflow: one-bash
fixture: fixtures/empty.yaml
expect:
  final_state: finished
"#,
    );

    let output = yunta_in!(&repo, &home, &["test"]);
    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(
        yunta_testkit::checked(&text, "wrong-expect")
            .is_some_and(|said| said.starts_with("failed: 1 error")),
        "the verdict counts the problems listed under it: {text}"
    );
    // The words a case file is written with, not a Rust enum's `Debug`.
    assert!(
        text.lines()
            .any(|l| l.trim_start() == "final_state: expected finished, got needs you"),
        "the mismatch names the field, the expected state and the actual one: {text}"
    );
    // And the reason sits on its own line under it, unquoted and
    // unescaped — it was written for a reader already.
    assert!(
        text.lines()
            .any(|l| l.trim_start().starts_with("node `fails` failed:")),
        "the reason reaches the reader intact: {text}"
    );
}

#[test]
fn a_run_under_test_reads_no_org_config_from_the_host() {
    // What the machine running the suite would hand the binary: an org
    // layer is a ceiling the layers under it can only narrow, so one
    // here decides what every run may do.
    const HOSTILE: &str = "permissions:\n  commands:\n    deny: [\"echo *\"]\n";
    const WORKFLOW: &str = r#"
name: bash-only
nodes:
  - id: greet
    kind: bash
    run: "echo hello > made.txt"
"#;

    let root = tempfile::tempdir().unwrap();
    let hostile = Checkout::under(&root.path().join("hostile"))
        .with_org_config(HOSTILE)
        .workflow("wf", WORKFLOW);
    let refused = yunta_at!(hostile, &["run", "wf.yaml"]);
    assert!(
        !refused.status.success(),
        "the org layer decides what a run may do: {}",
        stderr(&refused),
    );

    // The same workflow under the harness, which hands every run an org
    // layer of its own: what it reads is empty, not the machine's.
    let under_test = Checkout::under(&root.path().join("under-test")).workflow("wf", WORKFLOW);
    let output = yunta_at!(under_test, &["run", "wf.yaml"]);
    assert!(
        output.status.success(),
        "a run under test reads no org config from the host: {}",
        stderr(&output),
    );
}

#[test]
fn yunta_test_refuses_a_workflow_check_refuses() {
    // A case runs its workflow, so a workflow this binary would refuse
    // to run is refused before any case does — and the reader is told
    // what is wrong with the file, not that a file could not be loaded.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/broken.yaml"),
        r#"
name: broken
nodes:
  - { id: only, kind: bash, run: "true", depends_on: [ghost] }
"#,
    );
    write(
        &repo.join(".yunta/tests/never-runs.yaml"),
        "workflow: broken\nfixture: fixtures/none.yaml\nexpect: { final_state: finished }\n",
    );
    write(
        &repo.join(".yunta/tests/fixtures/none.yaml"),
        "dialect: claude-code\nsessions: []\n",
    );

    let out = yunta_in!(&repo, &home, &["test"]);
    assert!(!out.status.success(), "a broken workflow fails the suite");
    let text = format!("{}{}", stdout(&out), stderr(&out));
    assert!(
        text.contains("`ghost`"),
        "the refusal names what the workflow reaches for and does not have: {text}"
    );
}

#[test]
fn a_case_answers_a_gate_and_reaches_the_promotion_it_expects() {
    // `expect: promoted` is reachable because a case answers a gate the
    // way a person does: the run parks, the decision goes on its log,
    // and the run is handed back through the same consequence path.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join(".yunta/workflows/promotable.yaml"), PROMOTABLE);
    write(
        &repo.join(".yunta/tests/fixtures/none.yaml"),
        "dialect: claude-code\nsessions: []\n",
    );
    write(
        &repo.join(".yunta/tests/promotes.yaml"),
        r#"
workflow: promotable
mode: quick
fixture: fixtures/none.yaml
decisions:
  check: promote
expect:
  final_state: promoted
"#,
    );

    let out = yunta_in!(&repo, &home, &["test"]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        yunta_testkit::checked(&stdout(&out), "promotes").as_deref() == Some("ok"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn a_case_whose_fixture_describes_a_session_nobody_opened_fails() {
    // A fixture is the case's own account of what the run does. One
    // that scripts a session the run never opens describes a run
    // nobody made, and the case says so rather than passing.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/one-node.yaml"),
        "name: one-node\nnodes:\n  - id: touch\n    kind: bash\n    run: \"true\"\n",
    );
    write(
        &repo.join(".yunta/tests/fixtures/spare.yaml"),
        "dialect: claude-code\nsessions:\n  - outcome: \
         { type: completed, summary: \"nobody asked\" }\n",
    );
    write(
        &repo.join(".yunta/tests/spare.yaml"),
        "workflow: one-node\nfixture: fixtures/spare.yaml\nexpect:\n  final_state: finished\n",
    );

    let out = yunta_in!(&repo, &home, &["test"]);
    let text = stdout(&out);
    assert!(!out.status.success(), "{text}");
    assert!(
        text.contains("1 scripted session nothing opened") && text.contains("#1"),
        "the case names which script went unclaimed: {text}"
    );
}

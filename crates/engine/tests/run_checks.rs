//! Verification gates: baseline and coverage compares, findings gates, progress.md, the executor, command-permission denials, and events.jsonl.

use yunta_engine::{NodeState, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;

#[tokio::test]
async fn baseline_compare_passes_on_its_first_run_with_nothing_to_compare_against() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("marker.txt"), "ok").unwrap();

    let workflow = r#"
name: baseline-first-run
nodes:
  - id: no-regressions
    kind: check
    builtin: baseline_compare
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn baseline_compare_fails_when_a_previously_green_suite_turns_red() {
    let bench = Bench::new();
    // `cat marker.txt` exits 0 while the file exists — the first
    // `baseline_compare` node below captures that as the baseline.
    std::fs::write(bench.worktree.join("marker.txt"), "ok").unwrap();

    let workflow = r#"
name: baseline-regression
nodes:
  - id: capture
    kind: check
    builtin: baseline_compare
  - id: regress
    kind: bash
    run: "rm marker.txt"
    depends_on: [capture]
  - id: compare
    kind: check
    builtin: baseline_compare
    depends_on: [regress]
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("regression"),
            "expected a regression diagnostic, got: {reason}"
        ),
        other => panic!("expected the second baseline_compare to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn coverage_gate_passes_when_measured_coverage_meets_the_threshold() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("coverage.txt"), "lines: 92.5%\n").unwrap();

    let workflow = r#"
name: coverage-ok
nodes:
  - id: coverage
    kind: check
    builtin: coverage_gate
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_COVERAGE)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn coverage_gate_fails_when_measured_coverage_is_below_the_threshold() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("coverage.txt"), "lines: 40.0%\n").unwrap();

    let workflow = r#"
name: coverage-low
nodes:
  - id: coverage
    kind: check
    builtin: coverage_gate
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_COVERAGE)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                "node `coverage` failed: coverage 40% is below the 80% threshold"
            );
        }
        other => panic!("expected the coverage gate to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn findings_gate_fails_when_a_posted_finding_meets_max_severity() {
    let bench = Bench::new();

    let workflow = r#"
name: findings-gate
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
  - id: gate
    kind: check
    builtin: findings_gate
    max_severity: major
    depends_on: [review]
"#;

    let fixture = review_session(&[(
        "f1",
        "blocking",
        "Unchecked error",
        "src/lib.rs:10",
        "The Result is discarded.",
    )]);

    let (terminal, _) = bench.run(workflow, &fixture).await;
    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `gate` failed: 1 finding(s) at or above Major: f1"
        ),
        other => panic!("expected the gate to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn findings_gate_passes_when_no_finding_meets_max_severity() {
    let bench = Bench::new();

    let workflow = r#"
name: findings-gate-clean
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
  - id: gate
    kind: check
    builtin: findings_gate
    max_severity: blocking
    depends_on: [review]
"#;

    let fixture = review_session(&[("f1", "minor", "Style nit", "src/lib.rs:10", "Naming.")]);

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn progress_md_is_regenerated_at_run_dir_after_each_node_finished() {
    let bench = Bench::new();

    let workflow = r#"
name: two-nodes
nodes:
  - id: write
    kind: bash
    run: "touch out.txt"
    description: "Writes the output file"
  - id: verify
    kind: bash
    run: "test -f out.txt"
    depends_on: [write]
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let progress = std::fs::read_to_string(bench.run_dir().join("progress.md")).unwrap();
    assert_eq!(progress, "# Progress\n\n## Finished\n\n- **write** — Writes the output file\n  outcome: exit 0\n- **verify** — verify\n  outcome: exit 0\n\n## Failed\n\n_none_\n\n## Next\n\n_nothing pending_\n");
}

#[tokio::test]
async fn progress_md_lists_a_node_s_artifacts_after_it_finishes() {
    let bench = Bench::new();

    let workflow = r#"
name: findings-progress
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the changes."
    description: "Reviews the diff for issues"
    artifacts:
      produces:
        - { name: findings.yaml, kind: findings }
"#;

    let fixture = review_session(&[]);

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let progress = std::fs::read_to_string(bench.run_dir().join("progress.md")).unwrap();
    assert_eq!(progress, "# Progress\n\n## Finished\n\n- **review** — Reviews the diff for issues\n  outcome: reviewed\n  artifact: artifacts/findings.yaml\n\n## Failed\n\n_none_\n\n## Next\n\n_nothing pending_\n");
}

#[tokio::test]
async fn an_executor_node_completes_the_full_cycle_with_a_dependency_free_python_script() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/bin/sh
input=$(cat)
threshold=$(printf '%s' "$input" | sed -n 's/.*"threshold":\([0-9]*\).*/\1/p')
run_dir=$(printf '%s' "$input" | sed -n 's/.*"run":{"dir":"\([^"]*\)".*/\1/p')
if [ -z "$run_dir" ]; then
    printf '%s\n' 'run.dir must be present in stdin' >&2
    exit 1
fi
printf '{"summary": "threshold was %s"}\n' "$threshold"
exit 0
"#,
    );

    let workflow = r#"
name: executor-happy-path
nodes:
  - id: probe
    kind: executor
    executor: probe
    with:
      threshold: 80
"#;

    let (terminal, state) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.get("probe") {
        Some(NodeState::Finished { outcome, .. }) => {
            assert_eq!(outcome, "threshold was 80");
        }
        other => panic!("expected probe to finish, got {other:?}"),
    }
}

#[tokio::test]
async fn an_executor_node_that_exits_non_zero_fails_the_node_with_its_stderr() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/bin/sh
printf '%s\n' 'threshold not met' >&2
exit 1
"#,
    );

    let workflow = r#"
name: executor-failure
nodes:
  - id: probe
    kind: executor
    executor: probe
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                "node `probe` failed: executor `probe` exited 1: threshold not met"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn an_executor_node_that_exceeds_its_timeout_fails_with_a_diagnostic() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/bin/sh
exec tail -f /dev/null
"#,
    );

    let workflow = r#"
name: executor-timeout
nodes:
  - id: probe
    kind: executor
    executor: probe
    timeout_seconds: 1
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                "node `probe` failed: executor `probe` exceeded its 1s timeout"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn an_executor_node_referencing_an_unregistered_name_fails_with_a_diagnostic() {
    let bench = Bench::new();

    let workflow = r#"
name: executor-unregistered
nodes:
  - id: probe
    kind: executor
    executor: does-not-exist
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("skills.executors"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn a_template_built_command_that_violates_at_runtime_fails_the_node_citing_the_rule() {
    // The YAML text alone never matches the denied pattern — the
    // violation only exists after {{run.worktree}} renders. The static
    // scan can't see it; the runtime moment must.
    let bench = Bench::new();
    let marked = bench.worktree.join("forbidden-marker");
    std::fs::create_dir_all(&marked).unwrap();

    let workflow = r#"
name: runtime-violation
nodes:
  - id: sneaky
    kind: bash
    run: "ls {{run.worktree}}/forbidden-marker"
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_DENY)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("forbidden-marker") && reason.contains("denied"),
                "must cite the rule: {reason}"
            );
        }
        other => panic!("expected the run to pause on the violation, got {other:?}"),
    }
}

#[tokio::test]
async fn a_denied_hook_command_fails_the_node_even_with_on_failure_warn() {
    // Governance is not a hook outcome: `on_failure: warn` downgrades a
    // hook's own failure, never a permission violation — otherwise any
    // hook could opt out of the permission model.
    let bench = Bench::new();

    let workflow = r#"
name: hook-violation
nodes:
  - id: build
    kind: bash
    run: "true"
    hooks:
      before:
        - run: "echo forbidden-marker"
          on_failure: warn
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_DENY)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(reason, "node `build` failed: command `echo forbidden-marker` matches denied pattern `*forbidden-marker*` (permissions.commands.deny)");
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn a_denied_task_criterion_blocks_the_task_citing_the_rule() {
    let bench = Bench::new();

    let workflow = r#"
name: criterion-violation
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces:
        - { name: plan.yaml, kind: tasks }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Do the task."
"#;

    let fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("T001", "Task", "out.txt", "test -f forbidden-marker")
    ));

    let (terminal, _) = bench
        .run_with_config(workflow, &fixture, CONFIG_WITH_DENY)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("denied"),
                "the blocked task must cite the rule: {reason}"
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn a_node_with_network_false_is_never_blocked_by_the_engine() {
    // A test that documents the limit, not a bug:
    // `network: false` is declarative; the engine runs the command anyway.
    let bench = Bench::new();

    let workflow = r#"
name: network-declarative
nodes:
  - id: declared-offline
    kind: bash
    run: "echo simulating-a-network-call"
    network: false
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "network: false activates no sandbox — policy, not capability"
    );
}

#[tokio::test]
async fn a_denied_executor_path_fails_the_node_citing_the_rule() {
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        "#!/bin/sh\nprintf '%s\\n' '{}'\n",
    );

    let workflow = r#"
name: executor-denied
nodes:
  - id: probe
    kind: executor
    executor: probe
"#;

    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
skills:
  executors:
    - { name: probe, kind: binary, path: probe.py }
permissions:
  commands:
    deny: ["*probe.py"]
"#;

    let (terminal, _) = bench
        .run_with_config(workflow, "sessions: []", config)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                format!(
                    "node `probe` failed: command `{}` matches denied pattern `*probe.py` (permissions.commands.deny)",
                    bench.worktree.join("probe.py").display()
                )
            );
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn events_jsonl_is_written_at_run_dir_when_the_run_finishes() {
    let bench = Bench::new();

    let workflow = r#"
name: single-node
nodes:
  - id: only
    kind: bash
    run: "true"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let jsonl = std::fs::read_to_string(bench.run_dir().join("events.jsonl")).unwrap();
    let round_tripped: Vec<yunta_core::events::StoredEvent> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(yunta_engine::derive(&round_tripped), state);
    assert!(
        jsonl.contains("\"kind\":\"run_finished\""),
        "the closing event itself must be included in the export"
    );
}

#[tokio::test]
async fn events_jsonl_is_also_written_when_the_run_pauses() {
    let bench = Bench::new();

    let workflow = r#"
name: no-runner
nodes:
  - id: plan
    kind: prompt
    prompt: "plan it"
"#;

    let (terminal, _) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause, got {other:?}"),
    }

    let jsonl = std::fs::read_to_string(bench.run_dir().join("events.jsonl")).unwrap();
    assert!(
        jsonl.contains("\"kind\":\"run_paused\""),
        "a paused run's export must include the pause itself"
    );
}

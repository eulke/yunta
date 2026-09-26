//! Verification gates: baseline and coverage compares, findings gates, progress.md, the executor, command-permission denials, and events.jsonl.

use yunta_core::events::{BaselineOrigin, EventPayload, NodeEvent, RunEvent};
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::{baselines, ApproveEverything, Bench, MOCK_CONFIG};

mod common;
use common::*;

#[tokio::test]
async fn a_run_born_and_not_yet_woken_holds_no_baseline() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();

    let workflow = r#"
name: baseline-at-first-wake
nodes:
  - id: work
    kind: bash
    run: "true"
"#;

    // A birth states what the run is and what it holds. What its tree
    // already did is a measurement, and measuring is something an
    // invocation does — so a run that nobody has woken has none.
    bench
        .create(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;

    assert!(
        bench.events().iter().all(|event| !matches!(
            event.payload(),
            Some(EventPayload::Run(RunEvent::BaselineCaptured(_)))
        )),
        "a birth measures nothing: {:#?}",
        bench.events()
    );
}

/// The workflow every baseline test drives: one node, so what a log
/// holds before it is what the wake itself did.
const ONE_NODE: &str = r#"
name: baseline-at-first-wake
nodes:
  - id: work
    kind: bash
    run: "true"
"#;

#[tokio::test]
async fn the_first_wake_measures_the_baseline_before_any_node() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();
    bench
        .create(ONE_NODE, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    bench.wake().await;

    let events = bench.events();
    let measured = baselines(&events);
    assert_eq!(measured.len(), 1, "one measurement: {events:#?}");
    assert_eq!(measured[0].command, "cat marker.txt");
    assert_eq!(measured[0].results.exit_code, 0);
    assert_eq!(
        measured[0].origin,
        BaselineOrigin::Measured,
        "this run took the measurement itself"
    );

    let at = position_of(&events, |payload| {
        matches!(payload, EventPayload::Run(RunEvent::BaselineCaptured(_)))
    })
    .expect("the measurement is on the log");
    assert_eq!(
        events[at].node_id, None,
        "the measurement is the run's own fact, under no node"
    );
    assert!(
        position_of(&events, |payload| matches!(
            payload,
            EventPayload::Node(NodeEvent::Started(_))
        ))
        .is_some_and(|started| at < started),
        "the suite runs before the first node of the run: {events:#?}"
    );
    assert!(
        position_of(&events, |payload| matches!(
            payload,
            EventPayload::Run(RunEvent::Resumed(_))
        ))
        .is_none(),
        "a first wake is not a resume: {events:#?}"
    );
}

/// The log carries the tail of what the suite said; the bytes it hashes
/// stay with the run that measured, for a reader of a comparison.
#[tokio::test]
async fn the_measuring_run_keeps_everything_the_suite_wrote() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();
    let run_dir = bench
        .create(ONE_NODE, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    bench.wake().await;

    let kept = tokio::fs::read(yunta_engine::run_dir::baseline_capture(&run_dir))
        .await
        .expect("the run keeps everything the suite wrote");
    assert_eq!(kept, b"ok\n");
    assert_eq!(
        baselines(&bench.events())[0].hash,
        yunta_core::sha256_hex(&kept),
        "the hash on the log names the bytes the run kept"
    );
}

/// What `node` closed with, as its `node_finished` states it.
fn outcome_of(events: &[yunta_core::events::StoredEvent], node: &str) -> String {
    events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Finished(payload)))
                if event.node_id.as_ref().is_some_and(|id| id.as_str() == node) =>
            {
                Some(payload.outcome.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{node}` closed: {events:#?}"))
}

/// Where the first event whose payload satisfies `is` sits in `events`.
fn position_of(
    events: &[yunta_core::events::StoredEvent],
    is: impl Fn(&EventPayload) -> bool,
) -> Option<usize> {
    events
        .iter()
        .position(|event| event.payload().is_some_and(&is))
}

#[tokio::test]
async fn a_run_born_holding_documents_is_not_resumed_on_its_first_wake() {
    let bench = Bench::new();

    let workflow = r#"
name: born-holding
inputs:
  brief: { type: document, kind: findings }
nodes:
  - id: work
    kind: bash
    run: "true"
"#;
    let brief = bench.worktree.join("brief.yaml");
    tokio::fs::write(&brief, "findings: []\n").await.unwrap();

    let bench = bench.with_inputs(&[("brief", brief.to_str().expect("a utf-8 path"))]);
    bench.create(workflow, "sessions: []", MOCK_CONFIG).await;
    bench.wake().await;

    assert!(
        bench.events().iter().all(|event| !matches!(
            event.payload(),
            Some(EventPayload::Run(RunEvent::Resumed(_)))
        )),
        "a birth writes as many events as the run was born holding, and none of them is a wake: {:#?}",
        bench.events()
    );
}

/// One measurement per lineage means one per run, however many times an
/// invocation picks the run back up.
#[tokio::test]
async fn a_resumed_run_measures_nothing_again() {
    let bench = Bench::new();
    let suite_runs = bench
        .worktree
        .parent()
        .expect("the worktree sits in the bench's world")
        .join("suite-runs");
    let config = format!(
        "{MOCK_CONFIG}baseline:\n  suite: \"echo . >> {}\"\n",
        suite_runs.display()
    );

    // The gate has nobody to answer it on the first wake, so the run
    // pauses there; the second wake brings a surface that answers.
    let workflow = r#"
name: measured-once
nodes:
  - id: approve
    kind: gate
    assignee: lead
  - id: work
    kind: bash
    depends_on: [approve]
    run: "true"
"#;

    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []", &config)
        .await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    let RunReport { terminal, .. } = bench.wake_answering(&ApproveEverything::new("test")).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let measured = bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Run(RunEvent::BaselineCaptured(_)))
            )
        })
        .count();
    assert_eq!(measured, 1, "the second wake finds the measurement held");
    let ran = tokio::fs::read_to_string(&suite_runs)
        .await
        .expect("the suite ran at least once");
    assert_eq!(ran.lines().count(), 1, "and so never runs the suite again");
}

/// The suite a comparison runs is the invocation's to reuse: two
/// comparisons with nothing between them ask once, and the second says
/// where its answer came from.
#[tokio::test]
async fn two_baseline_compares_on_one_tree_run_the_suite_once_and_the_second_says_so() {
    let bench = Bench::new();
    let suite_runs = bench
        .worktree
        .parent()
        .expect("the worktree sits in the bench's world")
        .join("suite-runs");
    let config = format!(
        "{MOCK_CONFIG}baseline:\n  suite: \"echo . >> {}\"\n",
        suite_runs.display()
    );

    let workflow = r#"
name: compared-twice
nodes:
  - id: first
    kind: check
    builtin: baseline_compare
  - id: second
    kind: check
    builtin: baseline_compare
    depends_on: [first]
"#;

    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []", &config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.events();
    assert_eq!(
        outcome_of(&events, "first"),
        "no regression vs baseline (exit 0)"
    );
    assert_eq!(
        outcome_of(&events, "second"),
        "no regression vs baseline (exit 0, reused: same tree since an earlier compare)"
    );

    let ran = tokio::fs::read_to_string(&suite_runs)
        .await
        .expect("the suite ran");
    assert_eq!(
        ran.lines().count(),
        2,
        "the measurement on the first wake, and one comparison the two nodes share"
    );
}

#[tokio::test]
async fn the_first_baseline_compare_fails_on_a_regression_made_before_it() {
    let bench = Bench::new();
    // `cat marker.txt` exits 0 while the file is there — what the run
    // measures on its first wake, and what `regress` then breaks.
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();

    let workflow = r#"
name: baseline-regression
nodes:
  - id: regress
    kind: bash
    run: "rm marker.txt"
  - id: no-regressions
    kind: check
    builtin: baseline_compare
    depends_on: [regress]
"#;

    let RunReport { terminal, state: _ } = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("regression"),
            "expected a regression diagnostic, got: {reason}"
        ),
        other => panic!(
            "a run's only `baseline_compare` compares against the measurement its first \
             wake took, so it must catch the regression; got {other:?}"
        ),
    }
}

#[tokio::test]
async fn baseline_compare_passes_while_the_suite_keeps_passing() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();

    let workflow = r#"
name: baseline-green
nodes:
  - id: no-regressions
    kind: check
    builtin: baseline_compare
"#;

    let RunReport { terminal, state: _ } = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_BASELINE)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
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

    let RunReport { terminal, state: _ } = bench
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

    let RunReport { terminal, state: _ } = bench
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
      produces: [findings]
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

    let RunReport { terminal, state: _ } = bench.run(workflow, &fixture).await;
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
      produces: [findings]
  - id: gate
    kind: check
    builtin: findings_gate
    max_severity: blocking
    depends_on: [review]
"#;

    let fixture = review_session(&[("f1", "minor", "Style nit", "src/lib.rs:10", "Naming.")]);

    let RunReport { terminal, state: _ } = bench.run(workflow, &fixture).await;
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

    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let progress = std::fs::read_to_string(bench.run_dir().join("progress.md")).unwrap();
    assert_eq!(progress, "# Progress\n\n## Finished\n\n- **write** — Writes the output file\n  outcome: exit 0\n- **verify** — verify\n  outcome: exit 0\n\n## Failed\n\n_none_\n\n## Next\n\n_nothing pending_\n");
}

#[tokio::test]
async fn progress_md_names_a_node_that_failed_as_soon_as_it_fails() {
    // What a retry or a corrective node reads next has to know about the
    // failure it follows, even when nothing finished after it.
    let bench = Bench::new();
    let workflow = r#"
name: one-failure
nodes:
  - id: broken
    kind: bash
    run: "exit 3"
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let progress = tokio::fs::read_to_string(bench.run_dir().join("progress.md"))
        .await
        .unwrap();
    assert_eq!(
        progress,
        "# Progress\n\n## Finished\n\n_none yet_\n\n## Failed\n\n- **broken** — `exit 3`\n\n## Next\n\n_nothing pending_\n"
    );
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
      produces: [findings]
"#;

    let fixture = review_session(&[]);

    let RunReport { terminal, state: _ } = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // The artifact is named by what it is and by the bytes the run
    // holds, never by where the file happens to sit.
    let held = bench.accepted();
    assert_eq!(held.len(), 1, "{held:?}");
    let progress = std::fs::read_to_string(bench.run_dir().join("progress.md")).unwrap();
    assert_eq!(
        progress,
        format!(
            "# Progress\n\n## Finished\n\n- **review** — Reviews the diff for issues\n  \
             outcome: reviewed\n  artifact: findings · {}\n\n\
             ## Failed\n\n_none_\n\n## Next\n\n_nothing pending_\n",
            held[0].content_hash.abbreviated()
        )
    );
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

    let RunReport { terminal, state } = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.state("probe") {
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

    let RunReport { terminal, state: _ } = bench
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
async fn an_executor_node_that_exits_non_zero_saying_nothing_is_named_by_its_code_alone() {
    // A probe that signals only through its exit code is the ordinary
    // case; the node's failure names the code and stops there, with no
    // colon promising a reason the executor never gave.
    let bench = Bench::new();
    write_executable_script(
        &bench.worktree.join("probe.py"),
        r#"#!/bin/sh
exit 3
"#,
    );

    let workflow = r#"
name: executor-silent-failure
nodes:
  - id: probe
    kind: executor
    executor: probe
"#;

    let RunReport { terminal, state: _ } = bench
        .run_with_config(workflow, "sessions: []", CONFIG_WITH_EXECUTOR)
        .await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(reason, "node `probe` failed: executor `probe` exited 3");
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

    let RunReport { terminal, state: _ } = bench
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

    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
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

    let RunReport { terminal, state: _ } = bench
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

    let RunReport { terminal, state: _ } = bench
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
      produces: [tasks]
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

    let RunReport { terminal, state: _ } = bench
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

    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
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

    let RunReport { terminal, state: _ } = bench
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

    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let jsonl = std::fs::read_to_string(bench.run_dir().join("events.jsonl")).unwrap();
    let round_tripped: Vec<yunta_core::events::StoredEvent> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // The export carries the run's own close, which the report the run
    // hands back was taken just before: what has to round-trip is every
    // node's state, and then the close on top of it.
    let exported = yunta_engine::derive(&round_tripped);
    let states = |ledger: &yunta_core::events::NodeLedger| {
        ledger
            .iter()
            .map(|(id, record)| (id.clone(), record.state.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(states(&exported.nodes), states(&state.nodes));
    assert_eq!(
        exported.run.closed().map(|(terminal, _)| *terminal),
        Some(yunta_core::events::TerminalState::Done)
    );
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

    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
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

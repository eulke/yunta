//! Concurrent execution: parallel nodes, join:all / join:any groups, per-loop concurrency, and orphan resume across a crash.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::Adapter;
use yunta_core::{AdapterId, ConfigLayer, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{git, Bench, FixedClock, MOCK_CONFIG};

mod common;
use common::*;

#[tokio::test]
async fn independent_nodes_run_concurrently_up_to_max_parallel_nodes() {
    let bench = Bench::new();

    // Each node writes its marker and then waits until two markers exist —
    // a barrier that cannot clear unless two nodes run at once. It forces
    // the batch to overlap deterministically, with no wall clock: if the
    // scheduler ran them one at a time the barrier would never clear.
    let workflow_yaml = r#"
name: fan-out
nodes:
  - id: a
    kind: bash
    run: 'touch a.started; while :; do set -- *.started; [ "$#" -ge 2 ] && break; done'
  - id: b
    kind: bash
    run: 'touch b.started; while :; do set -- *.started; [ "$#" -ge 2 ] && break; done'
  - id: c
    kind: bash
    run: 'touch c.started; while :; do set -- *.started; [ "$#" -ge 2 ] && break; done'
"#;
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer =
        serde_norway::from_str("defaults:\n  max_parallel_nodes: 2\n").unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    assert_eq!(report.terminal, RunTerminal::Finished);

    assert_eq!(
        max_open_nodes(&bench.events()),
        2,
        "exactly max_parallel_nodes (2) nodes overlap at once, batch then batch"
    );
}

#[tokio::test]
async fn max_parallel_nodes_defaults_to_1_and_stays_fully_sequential() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: fan-out
nodes:
  - id: a
    kind: bash
    run: "true"
  - id: b
    kind: bash
    run: "true"
"#;
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config = ConfigLayer::default();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    assert_eq!(report.terminal, RunTerminal::Finished);

    assert_eq!(
        max_open_nodes(&bench.events()),
        1,
        "unset max_parallel_nodes stays fully sequential (default 1)"
    );
}

#[tokio::test]
async fn a_run_interrupted_mid_node_resumes_by_restarting_the_orphan() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: resumable
nodes:
  - id: only
    kind: bash
    run: "test -f present.txt"
"#;
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Simulate a crash mid-node: the log has node_started with no
    // terminal event — exactly what a killed engine leaves behind.
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: yunta_core::events::EventPayload::NodeStarted(
                    yunta_core::events::NodeStartedPayload { attempt: 1 },
                ),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    std::fs::write(bench.worktree.join("present.txt"), "here").unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::RunResumed(_))
        )),
        "resume must be recorded in the log"
    );
    // The orphan restarted as attempt 2.
    let last_start = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::NodeStarted(p)) => Some(p.attempt),
            _ => None,
        })
        .next_back();
    assert_eq!(last_start, Some(2));
}

#[tokio::test]
async fn a_node_with_on_interrupt_fail_if_uncertain_pauses_instead_of_restarting() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: uncertain-on-crash
nodes:
  - id: only
    kind: bash
    run: "test -f present.txt"
    on_interrupt: fail_if_uncertain
"#;
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Same simulated crash as the restart_node test: node_started with no
    // terminal event.
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: yunta_core::events::EventPayload::NodeStarted(
                    yunta_core::events::NodeStartedPayload { attempt: 1 },
                ),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    match report.terminal {
        RunTerminal::Paused { reason } => assert_eq!(reason, "node(s) `only` were running with no terminal event when the engine last stopped — `on_interrupt: fail_if_uncertain` refuses to guess whether they finished; verify manually before resuming"),
        other => panic!("expected Paused, got {other:?}"),
    }
    // Never restarted: no second node_started attempt was ever emitted.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let starts = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::NodeStarted(_))
            )
        })
        .count();
    assert_eq!(starts, 1, "fail_if_uncertain must never blindly restart");
}

#[tokio::test]
async fn a_blocked_task_fails_the_loop_and_pauses_the_run() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: blocked-task
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

    // One task whose criterion the executor never satisfies; with
    // DEFAULT_MAX_RETRIES=2 that's three executor sessions, then blocked.
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"Impossible\"\n    scope: [\"missing.txt\"]\n    criteria:\n      - cmd: \"test -f missing.txt\"\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - outcome: {{ type: completed, summary: "attempt 1" }}
  - outcome: {{ type: completed, summary: "attempt 2" }}
  - outcome: {{ type: completed, summary: "attempt 3" }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(
        state.tasks.get("T001"),
        Some(&yunta_core::events::TaskStatus::Blocked)
    );
}

#[tokio::test]
async fn a_parallel_group_with_join_all_finishes_when_every_child_finishes() {
    let bench = Bench::new();

    let workflow = r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
        kind: bash
        run: "touch load.txt"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["pre-launch", "write-docs", "load-test"] {
        assert!(
            matches!(state.nodes.get(id), Some(NodeState::Finished { .. })),
            "expected `{id}` finished, got {:?}",
            state.nodes.get(id)
        );
    }
    assert!(bench.worktree.join("docs.txt").exists());
    assert!(bench.worktree.join("load.txt").exists());
}

#[tokio::test]
async fn a_parallel_group_with_join_all_fails_if_any_child_fails() {
    let bench = Bench::new();

    let workflow = r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
        kind: bash
        run: "exit 1"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `pre-launch` failed: child `load-test` failed under join: all"
        ),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get("load-test"),
        Some(NodeState::Failed { .. })
    ));
}

#[tokio::test]
async fn a_parallel_group_with_join_any_completes_with_the_first_success_and_interrupts_the_rest() {
    let bench = Bench::new();

    let workflow = r#"
name: race
nodes:
  - id: race
    kind: parallel
    join: any
    nodes:
      - id: fast
        kind: bash
        run: "true"
      - id: slow
        kind: bash
        run: "tail -f /dev/null; touch slow-finished-fully.txt"
"#;

    let (terminal, state) = bench.run(workflow, "sessions: []").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("fast"),
        Some(NodeState::Finished { .. })
    ));
    // The slow sibling blocks forever and is interrupted the moment `fast`
    // wins, before its own `touch` can run: the run finishing at all proves
    // the loser was cut short, and the absent marker proves the cut landed
    // before it did any work — never a wall-clock race.
    assert!(!bench.worktree.join("slow-finished-fully.txt").exists());
}

#[tokio::test]
async fn resuming_a_crashed_parallel_group_never_re_runs_a_child_that_already_finished() {
    let bench = Bench::new();

    let workflow_yaml = r#"
name: pre-launch
nodes:
  - id: pre-launch
    kind: parallel
    join: all
    nodes:
      - id: write-docs
        kind: bash
        run: "touch docs.txt"
      - id: load-test
        kind: bash
        run: "test -f present.txt"
"#;
    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Simulate a crash mid-group: the parallel node and one child
    // (write-docs) finished; the other child (load-test) never started.
    for event in [
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("pre-launch".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("write-docs".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("write-docs".into()),
            payload: yunta_core::events::EventPayload::NodeFinished(
                yunta_core::events::NodeFinishedPayload {
                    outcome: "exit 0".to_string(),
                    tokens_used: Default::default(),
                },
            ),
        },
    ] {
        bench
            .storage
            .append(&event, &yunta_core::SystemClock)
            .unwrap();
    }
    // If write-docs re-ran, it would overwrite this — instead assert it
    // survives untouched, since a second `touch` would only prove nothing.
    std::fs::write(bench.worktree.join("docs.txt"), "original").unwrap();
    std::fs::write(bench.worktree.join("present.txt"), "here").unwrap();

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let write_docs_starts = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("write-docs")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::NodeStarted(_))
                )
        })
        .count();
    assert_eq!(
        write_docs_starts, 1,
        "an already-finished child must not restart on resume"
    );
}

#[tokio::test]
async fn eight_independent_tasks_at_concurrency_4_match_concurrency_1_state_and_commits() {
    // Same final state, same commit sequence, regardless of
    // concurrency — the batch mechanism integrates strictly in ledger
    // declaration order no matter how many tasks dispatch at once.
    let sequential = Bench::new();
    let workflow_seq = concurrency_workflow(1);
    let fixture_seq = eight_tasks_fixture(&sequential.run_dir().join("artifacts"));
    let (terminal_seq, state_seq) = sequential
        .run_with_config(&workflow_seq, &fixture_seq, CONCURRENCY_CONFIG)
        .await;
    assert_eq!(terminal_seq, RunTerminal::Finished);

    let parallel = Bench::new();
    let workflow_par = concurrency_workflow(4);
    let fixture_par = eight_tasks_fixture(&parallel.run_dir().join("artifacts"));
    let (terminal_par, state_par) = parallel
        .run_with_config(&workflow_par, &fixture_par, CONCURRENCY_CONFIG)
        .await;
    assert_eq!(terminal_par, RunTerminal::Finished);

    for n in 1..=8 {
        let id: yunta_core::TaskId = format!("task-{n}").parse().unwrap();
        assert_eq!(
            state_seq.tasks.get(&id),
            Some(&yunta_core::events::TaskStatus::Done)
        );
        assert_eq!(
            state_par.tasks.get(&id),
            state_seq.tasks.get(&id),
            "task-{n} status must match between concurrency levels"
        );
    }

    let commits_seq = commit_subjects(&sequential.worktree);
    let commits_par = commit_subjects(&parallel.worktree);
    assert_eq!(
        commits_seq.len(),
        8,
        "expected one commit per task, got {commits_seq:?}"
    );
    assert_eq!(
        commits_seq, commits_par,
        "the same ledger must produce the same commit sequence at any concurrency"
    );
    // Declaration order, not finishing order.
    let expected: Vec<String> = (1..=8)
        .map(|n| format!("task task-{n}: Write out-{n}"))
        .collect();
    assert_eq!(commits_seq, expected);
}

#[tokio::test]
async fn a_task_green_in_isolation_but_broken_by_a_sibling_s_integration_returns_to_ready() {
    // Task A always integrates cleanly. Task B's own criterion is
    // satisfied in isolation (its own worktree predates A's integration)
    // but is re-checked false once A's file exists on the tree B rebases
    // onto — exactly "pasa en su worktree pero rompe tras la integración
    // de otra". B must go back to `ready` without touching A.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: integration-conflict
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: 2
    prompt: "Read your task from the ledger and implement it."
"#;

    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Create a", "a.txt", "test -f a.txt"),
        task_yaml(
            "task-b",
            "Create b, require no a",
            "b.txt",
            "test -f b.txt && test ! -f a.txt"
        ),
    );

    let mut fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
        ledger,
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );
    // Several task-b sessions: the first attempt succeeds in isolation and
    // is rejected at integration (back to ready); the retried attempt(s)
    // are now genuinely red (a.txt is already on the integrated tree) and
    // exhaust run_task's own retries into a real Blocked.
    for _ in 0..(DEFAULT_MAX_RETRIES + 2) {
        fixture.push_str(
            "  - match_prompt_contains: \"task-b\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-b }\n",
        );
    }

    let (terminal, state) = bench
        .run_with_config(workflow, &fixture, CONCURRENCY_CONFIG)
        .await;

    // task-a must have succeeded and stayed succeeded, unaffected by
    // task-b's fate.
    assert_eq!(
        state.tasks.get("task-a"),
        Some(&yunta_core::events::TaskStatus::Done),
        "task-a stays Done regardless of task-b's fate"
    );
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let a_statuses: Vec<_> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::TaskStatusChanged(p))
                if p.task_id.as_str() == "task-a" =>
            {
                Some(p.new_status)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        a_statuses,
        vec![
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "task-a must reach Done exactly once and never regress"
    );

    let b_statuses: Vec<_> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::TaskStatusChanged(p))
                if p.task_id.as_str() == "task-b" =>
            {
                Some(p.new_status)
            }
            _ => None,
        })
        .collect();
    assert!(
        b_statuses.contains(&yunta_core::events::TaskStatus::Pending),
        "task-b's rejected integration must return it to Pending (ready), not Blocked: {b_statuses:?}"
    );
    // Confirms the ordering claimed above: Pending shows up strictly after
    // task-b's first Running, i.e. it really was reverted mid-flight.
    let first_running = b_statuses
        .iter()
        .position(|s| *s == yunta_core::events::TaskStatus::Running)
        .unwrap();
    let reverted = b_statuses
        .iter()
        .position(|s| *s == yunta_core::events::TaskStatus::Pending)
        .unwrap();
    assert!(reverted > first_running);

    // The run eventually gives up on task-b (it can never satisfy "no
    // a.txt" once a.txt is permanently integrated) — that's expected, not
    // a test bug: the point here is task-a's own success was untouched.
    match terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("task-b"),
            "the pause names the task the run gave up on: {reason}"
        ),
        other => panic!("expected the run to eventually pause on task-b, got {other:?}"),
    }
    assert_eq!(
        state.tasks.get("task-b"),
        Some(&yunta_core::events::TaskStatus::Blocked),
        "the run pauses because task-b exhausted its retries into Blocked"
    );
}

#[tokio::test]
async fn a_task_s_scope_is_checked_against_its_own_diff_never_a_sibling_s() {
    // Two independent tasks dispatched in the same batch; task-x's own
    // declared scope never mentions task-y's file. If scope were checked
    // against anything but task-x's own isolated diff, task-y's write
    // would spuriously violate it.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = concurrency_workflow(2);
    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-x", "x", "x.txt", "test -f x.txt"),
        task_yaml("task-y", "y", "y.txt", "test -f y.txt"),
    );
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n  - match_prompt_contains: \"task-x\"\n    effects:\n      - {{ path: x.txt, content: \"x\" }}\n    outcome: {{ type: completed, summary: did-x }}\n  - match_prompt_contains: \"task-y\"\n    effects:\n      - {{ path: y.txt, content: \"y\" }}\n    outcome: {{ type: completed, summary: did-y }}\n",
        artifacts_dir.display(),
        ledger,
    );

    let (terminal, state) = bench
        .run_with_config(&workflow, &fixture, CONCURRENCY_CONFIG)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("task-x"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get("task-y"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    for (task, forbidden) in [("task-x", "y.txt"), ("task-y", "x.txt")] {
        for event in &events {
            if let Some(yunta_core::events::EventPayload::ScopeChecked(p)) = event.payload() {
                if p.task_id.as_ref().map(|id| id.as_str()) == Some(task) {
                    assert!(
                        !p.diff
                            .iter()
                            .any(|path| path.to_string_lossy().contains(forbidden)),
                        "task `{task}`'s own scope check must never see `{forbidden}`: {:?}",
                        p.diff
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn killing_the_engine_mid_batch_and_resuming_only_reruns_the_orphan() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow: yunta_core::Workflow = serde_norway::from_str(&concurrency_workflow(2)).unwrap();
    let config: yunta_core::ConfigLayer = serde_norway::from_str(CONCURRENCY_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let ledger = format!(
        "tasks:\n{}{}",
        task_yaml("task-p", "p", "p.txt", "test -f p.txt"),
        task_yaml("task-q", "q", "q.txt", "test -f q.txt"),
    );
    std::fs::create_dir_all(&artifacts_dir).unwrap();
    std::fs::write(artifacts_dir.join("plan.yaml"), &ledger).unwrap();

    // Simulate the crash by hand-writing the log up through: plan already
    // registered, the loop started, task-p already Done and committed,
    // and task-q left `Running` with no terminal event — an orphan.
    git(&bench.worktree, &["checkout", "-b", "yunta/task/task-p/1"]);
    std::fs::write(bench.worktree.join("p.txt"), "p").unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "task task-p: p"]);
    git(&bench.worktree, &["checkout", "-"]);
    git(
        &bench.worktree,
        &["merge", "--ff-only", "yunta/task/task-p/1"],
    );

    for event in [
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::ArtifactWritten(
                yunta_core::events::ArtifactWrittenPayload {
                    path: "artifacts/plan.yaml".into(),
                    content_hash: yunta_core::sha256_hex(b"irrelevant"),
                    artifact_kind: None,
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::TaskRegistered(
                yunta_core::events::TaskRegisteredPayload {
                    task_id: "task-p".into(),
                    criteria: vec![],
                    scope: vec!["p.txt".to_string()],
                    depends_on: vec![],
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::TaskRegistered(
                yunta_core::events::TaskRegisteredPayload {
                    task_id: "task-q".into(),
                    criteria: vec![],
                    scope: vec!["q.txt".to_string()],
                    depends_on: vec![],
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("plan".into()),
            payload: yunta_core::events::EventPayload::NodeFinished(
                yunta_core::events::NodeFinishedPayload {
                    outcome: "planned".to_string(),
                    tokens_used: Default::default(),
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::NodeStarted(
                yunta_core::events::NodeStartedPayload { attempt: 1 },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-p".into(),
                    new_status: yunta_core::events::TaskStatus::Running,
                    caused_by: 1.into(),
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-q".into(),
                    new_status: yunta_core::events::TaskStatus::Running,
                    caused_by: 1.into(),
                },
            ),
        },
        yunta_core::events::EventDraft {
            run_id: bench.run_id.clone(),
            node_id: Some("implement".into()),
            payload: yunta_core::events::EventPayload::TaskStatusChanged(
                yunta_core::events::TaskStatusChangedPayload {
                    task_id: "task-p".into(),
                    new_status: yunta_core::events::TaskStatus::Done,
                    caused_by: 1.into(),
                },
            ),
        },
        // task-q never got a follow-up — orphaned Running, no p.txt-style
        // commit ever landed for it.
    ] {
        bench
            .storage
            .append(&event, &yunta_core::SystemClock)
            .unwrap();
    }

    let fixture = "sessions:\n  - match_prompt_contains: \"task-q\"\n    effects:\n      - { path: q.txt, content: \"q\" }\n    outcome: { type: completed, summary: did-q }\n";
    let adapter = yunta_adapters::MockAdapter::from_yaml(fixture).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));

    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    assert_eq!(report.terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let p_running_count = events
        .iter()
        .filter(|e| {
            matches!(e.payload(), Some(yunta_core::events::EventPayload::TaskStatusChanged(p)) if p.task_id.as_str() == "task-p" && p.new_status == yunta_core::events::TaskStatus::Running)
        })
        .count();
    assert_eq!(
        p_running_count, 1,
        "an already-Done task must never be re-dispatched on resume"
    );
    assert_eq!(
        report.state.tasks.get("task-q"),
        Some(&yunta_core::events::TaskStatus::Done),
        "the orphaned task must be re-run to completion"
    );
}

// --- loop/check cancellation under join: any --------------------------

#[tokio::test]
async fn a_join_any_race_cancels_a_slow_loop_child_when_a_sibling_wins() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: race-loop
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: race
    kind: parallel
    depends_on: [plan]
    join: any
    nodes:
      - id: quick
        kind: bash
        run: "true"
      - id: slow-loop
        kind: loop
        runner: executor
        until: all_tasks_complete
        prompt: "Implement your task."
"#;

    // The task session hangs and never returns on its own; only the race
    // cancelling the loser ends it. Were the loser not cancelled the run
    // would hang here, so it finishing at all is the proof the loop was cut
    // short — no wall-clock assertion needed.
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.yaml", content: "tasks:\n  - id: T001\n    title: \"slow\"\n    scope: [\"slow.txt\"]\n    criteria:\n      - cmd: \"test -f slow.txt\"\n" }}
    outcome: {{ type: completed, summary: "planned" }}
  - outcome: {{ type: hang }}
"#,
        artifacts = artifacts_dir.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert!(matches!(
        state.nodes.get("race"),
        Some(NodeState::Finished { .. })
    ));
    match state.nodes.get("slow-loop") {
        Some(NodeState::Failed { outcome, .. }) => {
            assert_eq!(
                *outcome,
                "interrupted: a sibling in this join: any group finished first"
            );
        }
        other => panic!("the losing loop must be recorded interrupted, got {other:?}"),
    }
}

#[tokio::test]
async fn a_join_any_race_cancels_a_slow_check_child_when_a_sibling_wins() {
    let bench = Bench::new();
    // The baseline suite blocks forever; the losing check ends only when
    // the race cancels it. If it were not cancelled the run would hang here,
    // so its finishing is the proof — never a wall-clock margin.
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: "tail -f /dev/null"
"#;
    let workflow = r#"
name: race-check
nodes:
  - id: race
    kind: parallel
    join: any
    nodes:
      - id: quick
        kind: bash
        run: "true"
      - id: slow-check
        kind: check
        builtin: baseline_compare
"#;

    let (terminal, state) = bench
        .run_with_config(workflow, "sessions: []\n", config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.get("slow-check") {
        Some(NodeState::Failed { outcome, .. }) => {
            assert_eq!(
                *outcome,
                "interrupted: a sibling in this join: any group finished first"
            );
        }
        other => panic!("the losing check must be recorded interrupted, got {other:?}"),
    }
}

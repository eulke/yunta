//! Concurrent execution: parallel nodes, join:all / join:any groups, per-loop concurrency, and orphan resume across a crash.

use yunta_engine::{NodeState, RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_testkit::{git, git_output, Bench};

mod common;
use common::*;
use yunta_core::events::{ArtifactEvent, NodeEvent, RunEvent, TaskEvent};

#[tokio::test]
async fn independent_nodes_run_concurrently_up_to_max_parallel_nodes() {
    let bench = Bench::new();

    // Each node writes its marker and then waits until two markers exist —
    // a barrier that cannot clear unless two nodes run at once. It forces
    // the batch to overlap deterministically, with no wall clock: if the
    // scheduler ran them one at a time the barrier would never clear.
    let workflow = r#"
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

    let RunReport { terminal, .. } = bench
        .run_with_config(
            workflow,
            "sessions: []",
            "defaults:\n  max_parallel_nodes: 2\n",
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(
        max_open_nodes(&bench.events()),
        2,
        "exactly max_parallel_nodes (2) nodes overlap at once, batch then batch"
    );
}

#[tokio::test]
async fn max_parallel_nodes_defaults_to_1_and_stays_fully_sequential() {
    let bench = Bench::new();

    let workflow = r#"
name: fan-out
nodes:
  - id: a
    kind: bash
    run: "true"
  - id: b
    kind: bash
    run: "true"
"#;

    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(
        max_open_nodes(&bench.events()),
        1,
        "unset max_parallel_nodes stays fully sequential (default 1)"
    );
}

/// Leaves `node` on the log the way a killed engine does: started, with
/// no terminal event after it.
fn orphan_a_node(bench: &Bench, node: &str) {
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some(node.into()),
                payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                    yunta_core::events::NodeStartedPayload::attempt(1),
                )),
            },
            &yunta_core::SystemClock,
        )
        .expect("the log takes the start a crash left behind");
}

#[tokio::test]
async fn a_run_interrupted_mid_node_resumes_by_restarting_the_orphan() {
    let bench = Bench::new();

    let workflow = r#"
name: resumable
nodes:
  - id: only
    kind: bash
    run: "test -f present.txt"
"#;

    let RunReport { terminal, .. } = bench
        .run_sabotaged(workflow, "sessions: []", |_run_dir| {
            orphan_a_node(&bench, "only");
            yunta_testkit::write(&bench.worktree.join("present.txt"), "here");
        })
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Run(RunEvent::Resumed(_)))
        )),
        "resume must be recorded in the log"
    );
    // The orphan restarted as attempt 2.
    let last_start = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Node(NodeEvent::Started(p))) => Some(p.attempt),
            _ => None,
        })
        .next_back();
    assert_eq!(last_start, Some(2));
}

#[tokio::test]
async fn a_node_with_on_interrupt_fail_if_uncertain_pauses_instead_of_restarting() {
    let bench = Bench::new();

    let workflow = r#"
name: uncertain-on-crash
nodes:
  - id: only
    kind: bash
    run: "test -f present.txt"
    on_interrupt: fail_if_uncertain
"#;

    let RunReport { terminal, .. } = bench
        .run_sabotaged(workflow, "sessions: []", |_run_dir| {
            // Same simulated crash as the restart_node test: node_started with no
            // terminal event.
            bench
                .storage
                .append(
                    &yunta_core::events::EventDraft {
                        run_id: bench.run_id.clone(),
                        node_id: Some("only".into()),
                        payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                            yunta_core::events::NodeStartedPayload::attempt(1),
                        )),
                    },
                    &yunta_core::SystemClock,
                )
                .unwrap();
        })
        .await;

    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(reason, "node(s) `only` were running with no terminal event when the engine last stopped — `on_interrupt: fail_if_uncertain` refuses to guess whether they finished; verify manually before resuming"),
        other => panic!("expected Paused, got {other:?}"),
    }
    // Never restarted: no second node_started attempt was ever emitted.
    let events = bench.events();
    let starts = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::Node(NodeEvent::Started(
                    _
                )))
            )
        })
        .count();
    assert_eq!(starts, 1, "fail_if_uncertain must never blindly restart");
}

#[tokio::test]
async fn a_blocked_task_fails_the_loop_and_pauses_the_run() {
    let bench = Bench::new();

    let workflow = r#"
name: blocked-task
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
"#;

    // One task whose criterion the executor never satisfies; with
    // DEFAULT_MAX_RETRIES=2 that's three executor sessions, then blocked.
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("T001", "Impossible", "missing.txt", "test -f missing.txt")
    ));
    for attempt in 1..=(DEFAULT_MAX_RETRIES + 1) {
        fixture.push_str(&format!(
            "  - outcome: {{ type: completed, summary: \"attempt {attempt}\" }}\n"
        ));
    }

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Blocked)
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

    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["pre-launch", "write-docs", "load-test"] {
        assert!(
            matches!(state.nodes.state(id), Some(NodeState::Finished { .. })),
            "expected `{id}` finished, got {:?}",
            state.nodes.state(id)
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

    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `pre-launch` failed: child `load-test` failed under join: all"
        ),
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.state("load-test"),
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

    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("fast"),
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
        run: "test -f present.txt"
"#;

    let RunReport { terminal, .. } = bench
        .run_sabotaged(workflow, "sessions: []", |_run_dir| {
            // Simulate a crash mid-group: the parallel node and one child
            // (write-docs) finished; the other child (load-test) never started.
            for event in [
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("pre-launch".into()),
                    payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        yunta_core::events::NodeStartedPayload::attempt(1),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("write-docs".into()),
                    payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        yunta_core::events::NodeStartedPayload::attempt(1),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("write-docs".into()),
                    payload: yunta_core::events::EventPayload::Node(NodeEvent::Finished(
                        yunta_core::events::NodeFinishedPayload::new(
                            "exit 0".to_string(),
                            Default::default(),
                        ),
                    )),
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
        })
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let write_docs_starts = events
        .iter()
        .filter(|e| {
            e.node_id.as_ref().map(|id| id.as_str()) == Some("write-docs")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        _
                    )))
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
    // concurrency — the batch mechanism integrates strictly in declaration
    // declaration order no matter how many tasks dispatch at once.
    let sequential = Bench::new();
    let workflow_seq = concurrency_workflow(1);
    let fixture_seq = eight_tasks_fixture();
    let RunReport {
        terminal: terminal_seq,
        state: state_seq,
    } = sequential.run(&workflow_seq, &fixture_seq).await;
    assert_eq!(terminal_seq, RunTerminal::Finished);

    let parallel = Bench::new();
    let workflow_par = concurrency_workflow(4);
    let fixture_par = eight_tasks_fixture();
    let RunReport {
        terminal: terminal_par,
        state: state_par,
    } = parallel.run(&workflow_par, &fixture_par).await;
    assert_eq!(terminal_par, RunTerminal::Finished);

    for n in 1..=8 {
        let id: yunta_core::TaskId = format!("task-{n}").parse().unwrap();
        assert_eq!(
            state_seq.tasks.status(&id),
            Some(yunta_core::events::TaskStatus::Done)
        );
        assert_eq!(
            state_par.tasks.status(&id),
            state_seq.tasks.status(&id),
            "task-{n} status must match between concurrency levels"
        );
    }

    let commits_seq = sequential.commit_subjects();
    let commits_par = parallel.commit_subjects();
    assert_eq!(
        commits_seq.len(),
        8,
        "expected one commit per task, got {commits_seq:?}"
    );
    assert_eq!(
        commits_seq, commits_par,
        "the same tasks document must produce the same commit sequence at any concurrency"
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

    let workflow = r#"
name: integration-conflict
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
    concurrency: 2
    prompt: "Read your task from the tasks document and implement it."
"#;

    let tasks = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Create a", "a.txt", "test -f a.txt"),
        task_yaml(
            "task-b",
            "Create b, require no a",
            "b.txt",
            "test -f b.txt && test ! -f a.txt"
        ),
    );

    let mut fixture = plan_session(&tasks);
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

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;

    // task-a must have succeeded and stayed succeeded, unaffected by
    // task-b's fate.
    assert_eq!(
        state.tasks.status("task-a"),
        Some(yunta_core::events::TaskStatus::Done),
        "task-a stays Done regardless of task-b's fate"
    );
    let events = bench.events();
    let a_statuses: Vec<_> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(p)))
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
            Some(yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(p)))
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
        state.tasks.status("task-b"),
        Some(yunta_core::events::TaskStatus::Blocked),
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

    let workflow = concurrency_workflow(2);
    let tasks = format!(
        "tasks:\n{}{}",
        task_yaml("task-x", "x", "x.txt", "test -f x.txt"),
        task_yaml("task-y", "y", "y.txt", "test -f y.txt"),
    );
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-x\"\n    effects:\n      - {{ path: x.txt, content: \"x\" }}\n    outcome: {{ type: completed, summary: did-x }}\n  - match_prompt_contains: \"task-y\"\n    effects:\n      - {{ path: y.txt, content: \"y\" }}\n    outcome: {{ type: completed, summary: did-y }}\n",
        plan_session(&tasks),
    );

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-x"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.status("task-y"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.events();
    for (task, forbidden) in [("task-x", "y.txt"), ("task-y", "x.txt")] {
        for event in &events {
            if let Some(yunta_core::events::EventPayload::Node(NodeEvent::ScopeChecked(p))) =
                event.payload()
            {
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

    let workflow = concurrency_workflow(2);
    let fixture = "sessions:\n  - match_prompt_contains: \"task-q\"\n    effects:\n      - { path: q.txt, content: \"q\" }\n    outcome: { type: completed, summary: did-q }\n";

    let RunReport { terminal, state } = bench
        .run_sabotaged(&workflow, fixture, |run_dir| {
            let tasks = format!(
                "tasks:\n{}{}",
                task_yaml("task-p", "p", "p.txt", "test -f p.txt"),
                task_yaml("task-q", "q", "q.txt", "test -f q.txt"),
            );
            let artifacts_dir = run_dir.join("artifacts");
            std::fs::create_dir_all(&artifacts_dir).unwrap();
            std::fs::write(artifacts_dir.join("plan.yaml"), &tasks).unwrap();
            // The bytes the crashed run accepted, where it kept them: the loop
            // reads its tasks from the run's own store, not from the view.
            let tasks_hash = yunta_core::sha256_hex(tasks.as_bytes());
            std::fs::create_dir_all(run_dir.join("objects")).unwrap();
            std::fs::write(run_dir.join("objects").join(tasks_hash.as_str()), &tasks).unwrap();

            // Simulate the crash by hand-writing the log up through: plan already
            // registered, the loop started, task-p already Done and committed,
            // and task-q left `Running` with no terminal event — an orphan.
            // The same branch the loop's own dispatch would have made for this
            // run's attempt at `task-p`, composed the one way the engine does.
            let task_p_branch = yunta_engine::unit_branch(
                &bench.run_id,
                &yunta_engine::UnitId::Task("task-p".into()),
                1,
            );
            git(&bench.worktree, &["checkout", "-b", &task_p_branch]);
            std::fs::write(bench.worktree.join("p.txt"), "p").unwrap();
            git(&bench.worktree, &["add", "-A"]);
            git(&bench.worktree, &["commit", "-q", "-m", "task task-p: p"]);
            git(&bench.worktree, &["checkout", "-"]);
            git(&bench.worktree, &["merge", "--ff-only", &task_p_branch]);

            for event in [
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("plan".into()),
                    payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        yunta_core::events::NodeStartedPayload::attempt(1),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("plan".into()),
                    // A log written before `artifact_accepted` existed: the
                    // fold reads it as the same artifact under the same hash.
                    payload: yunta_core::events::EventPayload::Artifacts(ArtifactEvent::Written(
                        yunta_core::events::ArtifactWrittenPayload {
                            path: "artifacts/plan.yaml".into(),
                            content_hash: tasks_hash.clone(),
                            artifact_kind: Some(yunta_core::ArtifactKind::Tasks),
                        },
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("plan".into()),
                    payload: yunta_core::events::EventPayload::Tasks(TaskEvent::Registered(
                        yunta_core::events::TaskRegisteredPayload {
                            task_id: "task-p".into(),
                            criteria: vec![],
                            scope: vec!["p.txt".into()],
                            depends_on: vec![],
                        },
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("plan".into()),
                    payload: yunta_core::events::EventPayload::Tasks(TaskEvent::Registered(
                        yunta_core::events::TaskRegisteredPayload {
                            task_id: "task-q".into(),
                            criteria: vec![],
                            scope: vec!["q.txt".into()],
                            depends_on: vec![],
                        },
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("plan".into()),
                    payload: yunta_core::events::EventPayload::Node(NodeEvent::Finished(
                        yunta_core::events::NodeFinishedPayload::new(
                            "planned".to_string(),
                            Default::default(),
                        ),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("implement".into()),
                    payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        yunta_core::events::NodeStartedPayload::attempt(1),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("implement".into()),
                    payload: yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(
                        yunta_core::events::TaskStatusChangedPayload::to(
                            "task-p".into(),
                            yunta_core::events::TaskStatus::Running,
                            1.into(),
                        ),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("implement".into()),
                    payload: yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(
                        yunta_core::events::TaskStatusChangedPayload::to(
                            "task-q".into(),
                            yunta_core::events::TaskStatus::Running,
                            1.into(),
                        ),
                    )),
                },
                yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("implement".into()),
                    payload: yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(
                        yunta_core::events::TaskStatusChangedPayload::to(
                            "task-p".into(),
                            yunta_core::events::TaskStatus::Done,
                            1.into(),
                        ),
                    )),
                },
                // task-q never got a follow-up — orphaned Running, no p.txt-style
                // commit ever landed for it.
            ] {
                bench
                    .storage
                    .append(&event, &yunta_core::SystemClock)
                    .unwrap();
            }
        })
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let p_running_count = events
        .iter()
        .filter(|e| {
            matches!(e.payload(), Some(yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(p))) if p.task_id.as_str() == "task-p" && p.new_status == yunta_core::events::TaskStatus::Running)
        })
        .count();
    assert_eq!(
        p_running_count, 1,
        "an already-Done task must never be re-dispatched on resume"
    );
    assert_eq!(
        state.tasks.status("task-q"),
        Some(yunta_core::events::TaskStatus::Done),
        "the orphaned task must be re-run to completion"
    );
}

// --- loop/check cancellation under join: any --------------------------

#[tokio::test]
async fn a_join_any_race_cancels_a_slow_loop_child_when_a_sibling_wins() {
    let bench = Bench::new();

    let workflow = r#"
name: race-loop
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
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
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("T001", "slow", "slow.txt", "test -f slow.txt")
    ));
    fixture.push_str("  - outcome: { type: hang }\n");

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert!(matches!(
        state.nodes.state("race"),
        Some(NodeState::Finished { .. })
    ));
    match state.nodes.state("slow-loop") {
        Some(NodeState::Failed { failure, .. }) => {
            assert_eq!(
                failure.to_string(),
                "interrupted: a sibling in this join: any group finished first"
            );
        }
        other => panic!("the losing loop must be recorded interrupted, got {other:?}"),
    }
}

#[tokio::test]
async fn a_join_any_race_cancels_a_slow_check_child_when_a_sibling_wins() {
    let bench = Bench::new();
    // The coverage command blocks forever; the losing check ends only when
    // the race cancels it. If it were not cancelled the run would hang here,
    // so its finishing is the proof — never a wall-clock margin.
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
coverage:
  cmd: "tail -f /dev/null"
  threshold: 80.0
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
        builtin: coverage_gate
"#;

    let RunReport { terminal, state } = bench
        .run_with_config(workflow, "sessions: []\n", config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.state("slow-check") {
        Some(NodeState::Failed { failure, .. }) => {
            assert_eq!(
                failure.to_string(),
                "interrupted: a sibling in this join: any group finished first"
            );
        }
        other => panic!("the losing check must be recorded interrupted, got {other:?}"),
    }
}

#[tokio::test]
async fn a_task_session_runs_on_the_model_and_agent_the_runner_resolved() {
    let bench = Bench::new();

    // Two runners on one adapter, each with its own model, and the
    // executor with a named agent: what every session mounts is then
    // readable apart, session by session.
    let config = r#"
runners:
  planner:
    - { adapter: mock, model: plan-model }
  executor:
    - { adapter: mock, model: task-model, agent: builder }
"#;
    let workflow = r#"
name: resolved-runner
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
"#;
    let fixture = format!(
        "capabilities: {{ run_tools: true, custom_agents: true }}\nsessions:\n{}{}",
        tasks_session(
            &format!(
                "tasks:\n{}",
                task_yaml("T001", "Write out.txt", "out.txt", "test -f out.txt")
            ),
            "planned",
        ),
        "  - effects:\n      - { path: out.txt, content: \"1\" }\n    \
         outcome: { type: completed, summary: \"did T001\" }\n",
    );

    let RunReport { terminal, .. } = bench.run_with_config(workflow, &fixture, config).await;
    let adapter = bench.mock();

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        adapter.models_seen(),
        vec![Some("plan-model".into()), Some("task-model".into())],
        "each session runs on the model its own runner resolved",
    );
    assert_eq!(
        adapter.agents_seen(),
        vec![None, Some("builder".into())],
        "the task session runs as the agent its runner named",
    );
}

#[tokio::test]
async fn a_loop_node_declaring_an_interpreted_artifact_is_refused_without_run_tools() {
    let bench = Bench::new();

    // The document a `findings` artifact holds reaches the engine
    // through the run tools and nowhere else, so a loop node that
    // declares one on an adapter without them is refused before any
    // session opens — not after one produced nothing.
    let workflow = r#"
name: findings-loop
nodes:
  - id: implement
    kind: loop
    runner: executor
    until: all_tasks_complete
    prompt: "Implement your task."
    artifacts:
      produces: [findings]
"#;

    let RunReport { terminal, state } = bench
        .run(
            workflow,
            "capabilities: { run_tools: false }\nsessions: []\n",
        )
        .await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("implement") {
        Some(NodeState::Failed { failure, .. }) => {
            let said = failure.to_string();
            assert!(
                said.contains("declares a `findings` artifact")
                    && said.contains("declares no `run_tools` capability"),
                "the refusal names the artifact and the missing capability, got {said}",
            );
        }
        other => panic!("the node must be refused before it opens a session, got {other:?}"),
    }
}

#[tokio::test]
async fn a_parallel_child_with_fail_if_uncertain_fails_instead_of_restarting() {
    let bench = Bench::new();

    // `audit` is the child that was running when the engine stopped.
    // Its work is not safe to repeat, so it says so.
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
      - id: audit
        kind: bash
        on_interrupt: fail_if_uncertain
        run: "touch audit-ran-again.txt"
"#;

    let RunReport { terminal, state } = bench
        .run_sabotaged(workflow, "sessions: []", |_run_dir| {
            // A crash mid-group: the group and `audit` both started, and
            // nothing recorded how `audit` ended.
            for node in ["pre-launch", "audit"] {
                bench
                    .storage
                    .append(
                        &yunta_core::events::EventDraft {
                            run_id: bench.run_id.clone(),
                            node_id: Some(node.into()),
                            payload: yunta_core::events::EventPayload::Node(NodeEvent::Started(
                                yunta_core::events::NodeStartedPayload::attempt(1),
                            )),
                        },
                        &yunta_core::SystemClock,
                    )
                    .unwrap();
            }
        })
        .await;

    assert!(
        !bench.worktree.join("audit-ran-again.txt").exists(),
        "a child that refuses to guess whether it finished never runs a second time",
    );
    match state.nodes.state("audit") {
        Some(NodeState::Failed { failure, .. }) => {
            assert!(
                failure.to_string().contains("fail_if_uncertain"),
                "the failure says which policy refused, got {failure}",
            );
        }
        other => panic!("the uncertain child is recorded failed, got {other:?}"),
    }
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
}

// --- quién recibe un árbol propio (M32 · D184) ----------------------------

/// Two nodes that run at once meet here before either writes, so both
/// capture their starting tree before the other has touched anything —
/// which is the only way the blame is mutual. The rendezvous is under
/// the run directory because that is the one place every node of a run
/// reaches whether or not it works in a tree of its own.
fn meet_then_write(mine: &str, marker: &str) -> String {
    format!(
        "mkdir -p {mine} \"{{{{run.dir}}}}/meet\"; touch \"{{{{run.dir}}}}/meet/{marker}\"; \
         while :; do set -- \"{{{{run.dir}}}}/meet\"/*; [ \"$#\" -ge 2 ] && break; done; \
         echo x > {mine}/out.txt"
    )
}

/// Each child writes only inside the globs it declared, and `check`
/// already proved those globs disjoint. Neither is answerable for what
/// the other wrote.
#[tokio::test]
async fn two_parallel_children_with_disjoint_scope_both_finish_clean() {
    let bench = Bench::new();
    let workflow = format!(
        r#"
name: sweep
nodes:
  - id: sweep
    kind: parallel
    join: all
    nodes:
      - id: sweep-a
        kind: bash
        scope: ["a/**"]
        run: '{}'
      - id: sweep-b
        kind: bash
        scope: ["b/**"]
        run: '{}'
"#,
        meet_then_write("a", "a"),
        meet_then_write("b", "b"),
    );

    let RunReport { terminal, state } = bench.run(&workflow, "sessions: []").await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "{:?}",
        state.nodes.state("sweep-a")
    );
    for id in ["sweep-a", "sweep-b"] {
        assert!(
            matches!(state.nodes.state(id), Some(NodeState::Finished { .. })),
            "expected `{id}` finished, got {:?}",
            state.nodes.state(id)
        );
    }
    assert!(bench.worktree.join("a/out.txt").exists(), "a's work landed");
    assert!(bench.worktree.join("b/out.txt").exists(), "b's work landed");
}

/// The same, for nodes the scheduler batched rather than a group the
/// author declared: `max_parallel_nodes` is what puts them together, and
/// neither answers for the other either.
#[tokio::test]
async fn fan_out_nodes_under_max_parallel_two_do_not_blame_each_other() {
    let bench = Bench::new();
    let workflow = format!(
        r#"
name: fan-out-scoped
nodes:
  - id: sweep-a
    kind: bash
    scope: ["a/**"]
    run: '{}'
  - id: sweep-b
    kind: bash
    scope: ["b/**"]
    run: '{}'
"#,
        meet_then_write("a", "a"),
        meet_then_write("b", "b"),
    );

    let RunReport { terminal, state } = bench
        .run_with_config(
            &workflow,
            "sessions: []",
            "defaults:\n  max_parallel_nodes: 2\n",
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["sweep-a", "sweep-b"] {
        assert!(
            matches!(state.nodes.state(id), Some(NodeState::Finished { .. })),
            "expected `{id}` finished, got {:?}",
            state.nodes.state(id)
        );
    }
}

/// A child that writes outside every glob it declared fails for its own
/// write, and takes nobody with it: the sibling that stayed inside its
/// scope finishes and lands.
#[tokio::test]
async fn a_parallel_child_that_writes_outside_every_scope_fails_alone() {
    let bench = Bench::new();
    let workflow = r#"
name: sweep
nodes:
  - id: sweep
    kind: parallel
    join: all
    nodes:
      - id: tidy
        kind: bash
        scope: ["a/**"]
        run: "mkdir -p a && echo x > a/out.txt && echo escaped > loose.txt"
      - id: neat
        kind: bash
        scope: ["b/**"]
        run: "mkdir -p b && echo x > b/out.txt"
"#;

    let RunReport { state, .. } = bench.run(workflow, "sessions: []").await;
    assert!(
        matches!(state.nodes.state("tidy"), Some(NodeState::Failed { .. })),
        "the node that wrote outside its scope fails, got {:?}",
        state.nodes.state("tidy")
    );
    assert!(
        matches!(state.nodes.state("neat"), Some(NodeState::Finished { .. })),
        "and the sibling that stayed inside its own finishes, got {:?}",
        state.nodes.state("neat")
    );
    assert!(
        !bench.worktree.join("loose.txt").exists(),
        "a unit that failed never lands, so its write never reaches the run's tree"
    );
}

/// Two units that landed on one branch left one history, not two heads:
/// the run's tree carries both, each as its own commit.
#[tokio::test]
async fn two_units_landing_on_one_branch_serialize() {
    let bench = Bench::new();
    let workflow = format!(
        r#"
name: sweep
nodes:
  - id: sweep
    kind: parallel
    join: all
    nodes:
      - id: sweep-a
        kind: bash
        scope: ["a/**"]
        run: '{}'
      - id: sweep-b
        kind: bash
        scope: ["b/**"]
        run: '{}'
"#,
        meet_then_write("a", "a"),
        meet_then_write("b", "b"),
    );

    let RunReport { terminal, .. } = bench.run(&workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let log = git_output(&bench.worktree, &["log", "--oneline", "--first-parent"]);
    let landed = log
        .lines()
        .filter(|line| line.contains("sweep-a") || line.contains("sweep-b"))
        .count();
    assert_eq!(landed, 2, "both units landed, one commit each:\n{log}");
    assert!(
        git_output(&bench.worktree, &["status", "--porcelain"])
            .trim()
            .is_empty(),
        "and the run's tree is clean once they have"
    );
}

/// A node that declares no scope keeps the run's own tree: what it
/// writes is there for the next node exactly as before, including what
/// git ignores — which a checkout of its own would never have carried.
#[tokio::test]
async fn a_node_without_scope_keeps_the_runs_own_tree() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join(".gitignore"), "build/\n")
        .await
        .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-qm", "ignore build"]);

    let workflow = r#"
name: ignored-output
nodes:
  - id: compile
    kind: bash
    run: "mkdir -p build && echo artifact > build/out.bin"
  - id: consume
    kind: bash
    depends_on: [compile]
    run: "test -f build/out.bin"
"#;

    let RunReport { terminal, state } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        matches!(
            state.nodes.state("consume"),
            Some(NodeState::Finished { .. })
        ),
        "what a node with no declared scope leaves is still there for the next one"
    );
}

/// A node with a tree of its own that the engine left running never
/// landed it, so the run's tree does not carry what it wrote. The
/// restart opens a fresh unit — its attempt is part of the name — over
/// the run's tree as it actually stands, and lands from there: what an
/// interrupted attempt left behind is never mistaken for work the run
/// accepted.
#[tokio::test]
async fn a_unit_whose_attempt_was_interrupted_restarts_over_the_tree_that_landed() {
    let bench = Bench::new();

    let workflow = r#"
name: resumable
nodes:
  - id: only
    kind: bash
    scope: ["out/**"]
    run: "mkdir -p out && echo second > out/done.txt"
"#;

    let RunReport { terminal, state } = bench
        .run_sabotaged(workflow, "sessions: []", |_run_dir| {
            orphan_a_node(&bench, "only");
        })
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        matches!(state.nodes.state("only"), Some(NodeState::Finished { .. })),
        "the orphan restarts and closes, got {:?}",
        state.nodes.state("only")
    );
    assert_eq!(
        yunta_testkit::read(&bench.worktree.join("out/done.txt")),
        "second\n",
        "the run's tree carries what the attempt that landed wrote"
    );
    assert!(
        git_output(&bench.worktree, &["status", "--porcelain"])
            .trim()
            .is_empty(),
        "and nothing of the interrupted attempt is lying in it"
    );
}

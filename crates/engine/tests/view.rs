//! [`run_frame`] — the snapshot every surface presents a run from.
//!
//! Each log here is built by hand with fixed timestamps and read at an
//! instant the test chooses, so no assertion depends on how long
//! anything takes or on what a clock says.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, AgentSessionOpenedPayload, ArtifactWrittenPayload,
    Capabilities, CapabilityDegradedPayload, ChildRunCreatedPayload, ChildRunFinishedPayload,
    DiscardedCandidate, EventBody, EventPayload, Failure, GateWaitingPayload, NodeFailedPayload,
    NodeFinishedPayload, NodeReroutedPayload, NodeStartedPayload, PromotionSignaledPayload,
    RerouteOrigin, RunCreatedPayload, RunFinishedPayload, RunMetrics, RunPausedPayload,
    RunnerResolvedPayload, StoredEvent, TaskRegisteredPayload, TaskStatus,
    TaskStatusChangedPayload, TerminalState, TokenUsage, UnknownEvent,
};
use yunta_core::{
    AgentName, Capability, CommitSha, ContentHash, ModeName, NodeId, NodeKind, RunnerCandidate,
    TaskId, Workflow,
};
use yunta_engine::{
    run_frame, Counter, NodeStanding, NodeState, Percentiles, PriorEstimation, RunFrame, RunPhase,
    WaitingOn,
};

const RUN: &str = "run-1";

fn base() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
}

fn at(offset_secs: i64) -> DateTime<Utc> {
    base() + chrono::Duration::seconds(offset_secs)
}

/// A log from `(offset, node, payload)` entries: storage numbers
/// positions from 1, and these follow declaration order.
fn log(entries: Vec<(i64, Option<&str>, EventPayload)>) -> Vec<StoredEvent> {
    entries
        .into_iter()
        .enumerate()
        .map(|(index, (offset, node, payload))| StoredEvent {
            run_id: RUN.into(),
            seq: (index as u64 + 1).into(),
            timestamp: at(offset),
            node_id: node.map(Into::into),
            body: EventBody::Known(payload),
        })
        .collect()
}

/// Appends an event written under a `kind` this binary does not know.
fn push_unknown(events: &mut Vec<StoredEvent>, offset: i64, kind: &str) {
    events.push(StoredEvent {
        run_id: RUN.into(),
        seq: (events.len() as u64 + 1).into(),
        timestamp: at(offset),
        node_id: None,
        body: EventBody::Unknown(UnknownEvent {
            kind: kind.to_string(),
            schema_version: 1,
            payload: serde_json::Map::new(),
        }),
    });
}

fn workflow(yaml: &str) -> Workflow {
    yunta_core::yaml::parse(yaml).unwrap()
}

fn chain() -> Workflow {
    workflow(
        r#"
name: ship
nodes:
  - { id: plan, kind: bash, run: "true" }
  - { id: build, kind: bash, run: "true", depends_on: [plan] }
  - { id: fix, kind: bash, run: "true" }
"#,
    )
}

fn frame(workflow: &Workflow, events: &[StoredEvent], now_secs: i64) -> RunFrame {
    run_frame(&RUN.into(), workflow, events, None, at(now_secs))
}

fn created(mode: &str) -> EventPayload {
    EventPayload::RunCreated(RunCreatedPayload {
        manifest_hash: ContentHash::sha256(b"manifest"),
        inputs: BTreeMap::new(),
        mode: mode.into(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: CommitSha::from("abc1234"),
    })
}

fn started(attempt: u32) -> EventPayload {
    EventPayload::NodeStarted(NodeStartedPayload { attempt })
}

fn tokens(input: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        output,
        cached: None,
    }
}

fn finished() -> EventPayload {
    EventPayload::NodeFinished(NodeFinishedPayload {
        outcome: "ok".to_string(),
        tokens_used: tokens(10, 5),
    })
}

fn failed(outcome: &str) -> EventPayload {
    EventPayload::NodeFailed(NodeFailedPayload::new(
        Failure::message(outcome.to_string()),
        true,
        tokens(7, 3),
    ))
}

fn rerouted(to: &str) -> EventPayload {
    EventPayload::NodeRerouted(NodeReroutedPayload {
        to_node: to.into(),
        cause: "criteria still red".to_string(),
        attempt: Some(1),
        max_reroutes: Some(2),
        origin: RerouteOrigin::OnFailure,
    })
}

fn registered(task: &str) -> EventPayload {
    EventPayload::TaskRegistered(TaskRegisteredPayload {
        task_id: task.into(),
        criteria: Vec::new(),
        scope: Vec::new(),
        depends_on: Vec::new(),
    })
}

fn task_now(task: &str, new_status: TaskStatus) -> EventPayload {
    EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
        task_id: task.into(),
        new_status,
        caused_by: 1.into(),
    })
}

fn session_opened(session: &str) -> EventPayload {
    EventPayload::AgentSessionOpened(AgentSessionOpenedPayload {
        session_id: session.into(),
        agent: Some("builder".into()),
        model: Some("mock-model".into()),
        capabilities: Capabilities::default(),
    })
}

fn tool_use(tool: &str) -> EventPayload {
    EventPayload::AgentMessage(AgentMessagePayload {
        message_type: AgentMessageType::ToolUse,
        tool_name: Some(tool.to_string()),
        target_digest: None,
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        text: None,
    })
}

fn run_finished(terminal_state: TerminalState) -> EventPayload {
    EventPayload::RunFinished(RunFinishedPayload {
        terminal_state,
        metrics: RunMetrics {
            cptv: None,
            tokens: tokens(10, 5),
        },
    })
}

/// Every bucket of `counter`, so a movement between two of them is read
/// as a whole rather than one field at a time.
fn buckets(counter: &Counter) -> [usize; 5] {
    [
        counter.done,
        counter.failed,
        counter.running,
        counter.waiting,
        counter.to_go,
    ]
}

// --- what the frame names ----------------------------------------------

#[test]
fn the_frame_names_the_run_and_the_workflow_its_manifest_froze() {
    let workflow = chain();
    let events = log(vec![(0, None, created("standard"))]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(frame.run_id, RUN);
    assert_eq!(
        frame.workflow, "ship",
        "the name is the frozen workflow's own, never one an event carries"
    );
}

#[test]
fn what_past_runs_cost_is_the_callers_to_hand_over_and_travels_unchanged() {
    let workflow = chain();
    let events = log(vec![(0, None, created("standard"))]);

    assert_eq!(
        run_frame(&RUN.into(), &workflow, &events, None, at(100)).prior,
        None,
        "a run's own log cannot know what this workflow's other runs cost"
    );

    let prior = PriorEstimation {
        sample_count: 12,
        tokens: Percentiles {
            median: 340_000.0,
            p90: 520_000.0,
        },
        wall_clock_secs: Some(Percentiles {
            median: 1_320.0,
            p90: 2_400.0,
        }),
        tasks: Percentiles {
            median: 8.0,
            p90: 14.0,
        },
    };
    assert_eq!(
        run_frame(&RUN.into(), &workflow, &events, Some(&prior), at(100)).prior,
        Some(prior)
    );
}

// --- the counters ------------------------------------------------------

#[test]
fn a_node_that_reroutes_and_runs_again_moves_between_buckets_without_shrinking_the_total() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (3, Some("build"), started(1)),
        (4, Some("build"), failed("criteria still red")),
        (5, Some("build"), rerouted("fix")),
        (6, Some("fix"), started(1)),
        (7, Some("fix"), finished()),
        (8, Some("build"), started(2)),
    ]);

    let mut totals = Vec::new();
    for prefix in 0..=events.len() {
        let flow = frame(&workflow, &events[..prefix], 100).flow;
        assert_eq!(
            buckets(&flow).iter().sum::<usize>(),
            flow.total,
            "the buckets partition the total at every point of the log"
        );
        totals.push(flow.total);
    }
    assert!(
        totals.windows(2).all(|pair| pair[1] >= pair[0]),
        "a re-route never shrinks the denominator: {totals:?}"
    );

    // The re-run of a failed node is a move out of `failed` and into
    // `running`, never a count that drops on its own.
    let before = frame(&workflow, &events[..8], 100).flow;
    let after = frame(&workflow, &events, 100).flow;
    assert_eq!(buckets(&before), [2, 1, 0, 0, 0]);
    assert_eq!(buckets(&after), [2, 0, 1, 0, 0]);
    assert_eq!(after.total, before.total);
    assert_eq!(after.skipped, 0);
    assert_eq!(after.skipped_by, None);
}

#[test]
fn the_run_counts_every_reroute_and_the_node_keeps_the_last_one_it_took() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), failed("red")),
        (3, Some("build"), rerouted("fix")),
    ]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(frame.reroutes, 1);
    let build = frame
        .nodes
        .iter()
        .find(|node| node.id == "build")
        .expect("the frozen dag declares it");
    let reroute = build.reroute.as_ref().expect("the log recorded one");
    assert_eq!(reroute.to, "fix");
    assert_eq!(reroute.attempt, Some(1));
    assert_eq!(reroute.max, Some(2));
    assert_eq!(reroute.origin, RerouteOrigin::OnFailure);
    assert_eq!(reroute.at, at(3));
}

#[test]
fn tasks_are_absent_until_a_ledger_registers_one() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), registered("t1")),
        (3, Some("build"), registered("t2")),
        (4, Some("build"), task_now("t1", TaskStatus::Done)),
    ]);
    assert_eq!(
        frame(&workflow, &events[..2], 100).tasks,
        None,
        "a run with no ledger reports no ledger, never 0/0"
    );

    let tasks = frame(&workflow, &events, 100)
        .tasks
        .expect("the ledger registered two tasks");
    assert_eq!(buckets(&tasks), [1, 0, 0, 0, 1]);
    assert_eq!(tasks.total, 2);
    assert_eq!(tasks.skipped, 0);
    assert_eq!(tasks.skipped_by, None);
}

#[test]
fn a_task_that_returns_to_ready_moves_back_into_to_go_and_the_total_holds() {
    // A task that fails at integration returns to `ready`
    // (`contrato-del-run.md` §5.5): the counter shows the move, and the
    // denominator does not budge.
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), registered("t1")),
        (3, Some("build"), task_now("t1", TaskStatus::Running)),
        (4, Some("build"), task_now("t1", TaskStatus::Ready)),
    ]);
    let running = frame(&workflow, &events[..4], 100)
        .tasks
        .expect("registered");
    assert_eq!(buckets(&running), [0, 0, 1, 0, 0]);

    let back = frame(&workflow, &events, 100).tasks.expect("registered");
    assert_eq!(buckets(&back), [0, 0, 0, 0, 1]);
    assert_eq!(back.total, running.total);
}

#[test]
fn a_task_held_by_its_dependencies_is_waiting_and_a_failed_one_is_failed() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), registered("t1")),
        (3, Some("build"), registered("t2")),
        (4, Some("build"), registered("t3")),
        (5, Some("build"), task_now("t1", TaskStatus::Blocked)),
        (6, Some("build"), task_now("t2", TaskStatus::Failed)),
    ]);

    let tasks = frame(&workflow, &events, 100).tasks.expect("registered");
    assert_eq!(
        buckets(&tasks),
        [0, 1, 0, 1, 1],
        "a task parked on its dependencies is waiting, not running"
    );
    assert_eq!(tasks.total, 3);
}

// --- what a mode narrows -----------------------------------------------

fn moded() -> Workflow {
    workflow(
        r#"
name: ship
modes:
  quick: { include: [plan, fan] }
  full: { include: all }
nodes:
  - { id: plan, kind: bash, run: "true" }
  - id: fan
    kind: parallel
    nodes:
      - { id: left, kind: bash, run: "true" }
      - { id: right, kind: bash, run: "true" }
  - { id: audit, kind: bash, run: "true" }
"#,
    )
}

#[test]
fn a_modes_excluded_nodes_count_as_skipped_and_name_the_mode() {
    let workflow = moded();
    let events = log(vec![(0, None, created("quick"))]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(frame.mode, "quick");
    assert_eq!(frame.flow.skipped, 1);
    assert_eq!(frame.flow.skipped_by, Some(ModeName::from("quick")));
    assert_eq!(
        frame.flow.total, 4,
        "the denominator is what this run will do: the excluded node is outside it"
    );
    let audit = frame
        .nodes
        .iter()
        .find(|node| node.id == "audit")
        .expect("declared");
    assert_eq!(audit.state, NodeStanding::Skipped);
}

#[test]
fn a_parallel_groups_children_are_included_with_their_group() {
    // `modes:` names top-level nodes; a group the mode schedules runs
    // every child it declares, so no child of `fan` is skipped.
    let workflow = moded();
    let events = log(vec![(0, None, created("quick"))]);

    let frame = frame(&workflow, &events, 100);
    for child in ["left", "right"] {
        let node = frame
            .nodes
            .iter()
            .find(|node| node.id == child)
            .expect("declared inside the group");
        assert_eq!(node.state, NodeStanding::ToGo);
        assert_eq!(node.group, Some(NodeId::from("fan")));
    }
}

#[test]
fn a_mode_that_narrows_nothing_skips_nothing() {
    let workflow = moded();
    let events = log(vec![(0, None, created("full"))]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(frame.flow.skipped, 0);
    assert_eq!(frame.flow.skipped_by, None);
    assert_eq!(frame.flow.total, 5);
}

// --- what the log carries about each node ------------------------------

#[test]
fn every_node_names_the_kind_the_frozen_workflow_declares_for_it() {
    // The kind is the manifest's, never an event's: none of these nodes
    // has run, and each still names what it is.
    let workflow = workflow(
        r#"
name: shapes
nodes:
  - { id: plan, kind: prompt, prompt: "do it" }
  - { id: script, kind: bash, run: "true" }
  - { id: implement, kind: loop, until: all_tasks_complete, prompt: "do it" }
  - id: fan
    kind: parallel
    nodes:
      - { id: child, kind: bash, run: "true" }
  - { id: regression, kind: check, builtin: baseline_compare }
  - { id: lint, kind: executor, executor: linter }
  - { id: approve, kind: gate, assignee: lead }
  - { id: sub, kind: workflow, use: child }
"#,
    );
    let events = log(vec![(0, None, created("standard"))]);

    let kinds: Vec<&str> = frame(&workflow, &events, 100)
        .nodes
        .iter()
        .map(|node| node.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            "prompt", "bash", "loop", "parallel", "bash", "check", "executor", "gate", "workflow",
        ],
        "declaration order, each `parallel` group followed by its children"
    );
    for kind in NodeKind::KINDS {
        assert!(
            kinds.contains(kind),
            "`{kind}` is a kind a node can declare, and a frame spells it as the schema does"
        );
    }
}

#[test]
fn a_node_carries_every_artifact_it_wrote_in_log_order() {
    let workflow = chain();
    let wrote = |path: &str| {
        EventPayload::ArtifactWritten(ArtifactWrittenPayload {
            path: PathBuf::from(path),
            content_hash: ContentHash::sha256(path.as_bytes()),
            artifact_kind: None,
        })
    };
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), wrote("plan.md")),
        (3, Some("build"), wrote("report.md")),
        (4, Some("plan"), started(1)),
    ]);

    let frame = frame(&workflow, &events, 100);
    let artifacts = |id: &str| {
        frame
            .nodes
            .iter()
            .find(|node| node.id == id)
            .expect("declared")
            .artifacts
            .clone()
    };
    assert_eq!(
        artifacts("build"),
        vec![PathBuf::from("plan.md"), PathBuf::from("report.md")]
    );
    assert_eq!(
        artifacts("plan"),
        Vec::<PathBuf>::new(),
        "a node that wrote nothing carries nothing"
    );
}

#[test]
fn a_runner_that_fell_back_carries_the_candidates_it_passed_over() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (
            2,
            Some("build"),
            EventPayload::RunnerResolved(RunnerResolvedPayload {
                runner: "implementer".into(),
                chosen: RunnerCandidate {
                    adapter: "mock".into(),
                    model: "mock-model".into(),
                    agent: Some("builder".into()),
                },
                discarded: vec![DiscardedCandidate {
                    candidate: RunnerCandidate {
                        adapter: "absent".into(),
                        model: "big-model".into(),
                        agent: None,
                    },
                    reason: "adapter `absent` is not installed".to_string(),
                }],
            }),
        ),
    ]);

    let frame = frame(&workflow, &events, 100);
    let build = frame
        .nodes
        .iter()
        .find(|node| node.id == "build")
        .expect("declared");
    let runner = build.runner.as_ref().expect("the log resolved one");
    assert_eq!(runner.runner, "implementer");
    assert_eq!(runner.chosen.adapter, "mock");
    assert_eq!(runner.chosen.model, "mock-model");
    assert_eq!(runner.chosen.agent, Some(AgentName::from("builder")));
    assert_eq!(runner.discarded.len(), 1);
    assert_eq!(
        runner.discarded[0].reason,
        "adapter `absent` is not installed"
    );
}

#[test]
fn the_run_total_carries_the_work_in_flight_and_a_node_carries_its_closed_attempts() {
    let workflow = chain();
    let usage = EventPayload::AgentMessage(AgentMessagePayload {
        message_type: AgentMessageType::Usage,
        tool_name: None,
        target_digest: None,
        input_tokens: Some(100),
        output_tokens: Some(50),
        cached_input_tokens: None,
        text: None,
    });
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), usage),
    ]);

    let in_flight = frame(&workflow, &events, 100);
    assert_eq!(in_flight.tokens, tokens(100, 50));
    let build = in_flight
        .nodes
        .iter()
        .find(|node| node.id == "build")
        .expect("declared");
    assert_eq!(
        build.tokens,
        TokenUsage::default(),
        "a node reports what its closed attempts spent; this one has closed none"
    );
}

#[test]
fn a_loop_nodes_sessions_and_tool_calls_are_the_nodes_own() {
    // The event envelope names a node and never a session, so a loop
    // node running two task sessions at once reports one stream of tool
    // calls under the node — the limit is the schema's.
    let workflow = workflow(
        r#"
name: ship
nodes:
  - { id: implement, kind: loop, until: all_tasks_complete, prompt: "do it" }
"#,
    );
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("implement"), started(1)),
        (2, Some("implement"), session_opened("s-1")),
        (3, Some("implement"), session_opened("s-2")),
        (4, Some("implement"), registered("t1")),
        (5, Some("implement"), registered("t2")),
        (6, Some("implement"), task_now("t1", TaskStatus::Running)),
        (7, Some("implement"), tool_use("rg")),
        (8, Some("implement"), tool_use("cargo")),
    ]);

    let frame = frame(&workflow, &events, 100);
    let node = frame.nodes.first().expect("declared");
    let sessions: Vec<&str> = node
        .sessions
        .iter()
        .map(|session| session.session_id.as_str())
        .collect();
    assert_eq!(sessions, vec!["s-1", "s-2"], "oldest first");
    let calls: Vec<Option<&str>> = node
        .activity
        .iter()
        .map(|call| call.tool_name.as_deref())
        .collect();
    assert_eq!(
        calls,
        vec![Some("cargo"), Some("rg")],
        "newest first, under the node rather than under either session"
    );
    assert_eq!(node.running_tasks, vec![TaskId::from("t1")]);
}

#[test]
fn a_degraded_capability_is_carried_with_what_happened_instead() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (
            2,
            Some("build"),
            EventPayload::CapabilityDegraded(CapabilityDegradedPayload {
                capability: Capability::ResumeSession,
                adapter: "mock".into(),
                policy_applied: "restart_node".to_string(),
            }),
        ),
    ]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(frame.degraded.len(), 1);
    assert_eq!(frame.degraded[0].capability, Capability::ResumeSession);
    assert_eq!(frame.degraded[0].adapter, "mock");
    assert_eq!(frame.degraded[0].policy, "restart_node");
    assert_eq!(frame.degraded[0].node, Some(NodeId::from("build")));
    assert_eq!(frame.degraded[0].at, at(2));
}

// --- the instant the frame is read at ----------------------------------

#[test]
fn elapsed_and_last_event_age_are_measured_against_the_injected_now() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (10, Some("build"), started(1)),
    ]);

    let early = frame(&workflow, &events, 70);
    let late = frame(&workflow, &events, 610);

    assert_eq!(early.elapsed, Some(Duration::from_secs(70)));
    assert_eq!(late.elapsed, Some(Duration::from_secs(610)));

    let node_at = |frame: &RunFrame| {
        let build = frame
            .nodes
            .iter()
            .find(|node| node.id == "build")
            .expect("declared");
        (build.elapsed, build.last_event_age, build.attempt)
    };
    assert_eq!(
        node_at(&early),
        (
            Some(Duration::from_secs(60)),
            Some(Duration::from_secs(60)),
            Some(1)
        )
    );
    assert_eq!(
        node_at(&late),
        (
            Some(Duration::from_secs(600)),
            Some(Duration::from_secs(600)),
            Some(1)
        )
    );
}

#[test]
fn a_node_that_never_started_reports_no_elapsed_rather_than_zero() {
    let workflow = chain();
    let events = log(vec![(0, None, created("standard"))]);

    let frame = frame(&workflow, &events, 100);
    let plan = frame.nodes.first().expect("declared");
    assert_eq!(plan.state, NodeStanding::ToGo);
    assert_eq!(plan.elapsed, None);
    assert_eq!(plan.last_event_age, None);
    assert_eq!(plan.attempt, None);
}

// --- composition -------------------------------------------------------

#[test]
fn a_child_run_is_a_link_and_never_a_number_in_the_parents_counters() {
    let workflow = workflow(
        r#"
name: parent
nodes:
  - { id: sub, kind: workflow, use: child }
"#,
    );
    let hash = ContentHash::sha256(b"child workflow");
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("sub"), started(1)),
        (
            2,
            Some("sub"),
            EventPayload::ChildRunCreated(ChildRunCreatedPayload {
                child_run_id: "run-child".into(),
                child_workflow_hash: hash.clone(),
            }),
        ),
        (
            3,
            Some("sub"),
            EventPayload::ChildRunFinished(ChildRunFinishedPayload {
                child_run_id: "run-child".into(),
                child_workflow_hash: hash,
                terminal_state: TerminalState::Done,
                tokens: tokens(40, 20),
            }),
        ),
    ]);

    let open = frame(&workflow, &events[..3], 100);
    assert_eq!(open.children.len(), 1);
    assert_eq!(open.children[0].run_id, "run-child");
    assert_eq!(open.children[0].node, Some(NodeId::from("sub")));
    assert_eq!(
        open.children[0].terminal, None,
        "an open child has no terminal state yet"
    );
    assert_eq!(
        open.flow.total, 1,
        "the parent counts its own nodes; a child's graph is the child's"
    );
    assert_eq!(buckets(&open.flow), [0, 0, 1, 0, 0]);

    let closed = frame(&workflow, &events, 100);
    assert_eq!(closed.children.len(), 1, "the link is closed, not doubled");
    assert_eq!(closed.children[0].terminal, Some(TerminalState::Done));
    assert_eq!(closed.flow.total, 1);
}

#[test]
fn a_child_whose_birth_the_log_does_not_carry_is_linked_by_its_close() {
    // A log truncated or written by an older engine can carry a child's
    // close without its `child_run_created`: the link exists, already
    // closed, rather than the child going unreported.
    let workflow = workflow(
        r#"
name: parent
nodes:
  - { id: sub, kind: workflow, use: child }
"#,
    );
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("sub"), started(1)),
        (
            2,
            Some("sub"),
            EventPayload::ChildRunFinished(ChildRunFinishedPayload {
                child_run_id: "run-child".into(),
                child_workflow_hash: ContentHash::sha256(b"child workflow"),
                terminal_state: TerminalState::Failed,
                tokens: tokens(40, 20),
            }),
        ),
    ]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(frame.children.len(), 1);
    assert_eq!(frame.children[0].run_id, "run-child");
    assert_eq!(frame.children[0].node, Some(NodeId::from("sub")));
    assert_eq!(frame.children[0].terminal, Some(TerminalState::Failed));
}

// --- a log this binary only half understands ---------------------------

#[test]
fn unknown_event_kinds_survive_into_the_frame() {
    let workflow = chain();
    let mut events = log(vec![
        (0, None, created("standard")),
        (1, Some("plan"), started(1)),
    ]);
    push_unknown(&mut events, 2, "telemetry_sampled");
    push_unknown(&mut events, 3, "telemetry_sampled");
    push_unknown(&mut events, 4, "quota_checked");

    let frame = frame(&workflow, &events, 100);
    let carried: Vec<(&str, usize)> = frame
        .unknown_kinds
        .iter()
        .map(|unknown| (unknown.kind.as_str(), unknown.events))
        .collect();
    assert_eq!(
        carried,
        vec![("quota_checked", 1), ("telemetry_sampled", 2)]
    );
    assert_eq!(
        frame.phase,
        RunPhase::Running,
        "a kind this binary does not know leaves the run interpretable"
    );
}

// --- the run's phase ---------------------------------------------------

#[test]
fn a_node_parked_on_a_published_gate_is_what_the_phase_names() {
    let workflow = workflow(
        r#"
name: ship
nodes:
  - { id: approve, kind: gate, assignee: lead }
"#,
    );
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("approve"), started(1)),
        (
            2,
            Some("approve"),
            EventPayload::GateWaiting(GateWaitingPayload {
                summary: "approve the plan".to_string(),
                evidence: "the plan".to_string(),
                options: Vec::new(),
                external_ref: Some("https://forge/pr/1".to_string()),
            }),
        ),
        (
            3,
            None,
            EventPayload::RunPaused(RunPausedPayload {
                reason: "waiting on external gate: https://forge/pr/1".to_string(),
            }),
        ),
    ]);

    let frame = frame(&workflow, &events, 100);
    assert_eq!(
        frame.phase,
        RunPhase::Waiting {
            on: WaitingOn::Node {
                node: "approve".into(),
                external_ref: Some("https://forge/pr/1".to_string()),
            }
        }
    );
    assert_eq!(buckets(&frame.flow), [0, 0, 0, 1, 0]);
}

#[test]
fn a_paused_run_with_no_parked_node_names_the_reason_the_log_recorded() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), failed("out of budget")),
        (
            3,
            None,
            EventPayload::RunPaused(RunPausedPayload {
                reason: "token budget exceeded".to_string(),
            }),
        ),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: "token budget exceeded".to_string(),
            }
        }
    );
}

#[test]
fn a_failed_run_carries_the_failure_its_last_node_failed_recorded() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), failed("criteria still red")),
        (3, None, run_finished(TerminalState::Failed)),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Failed {
            failure: Some(Failure::message("criteria still red".to_string())),
        }
    );
}

#[test]
fn a_promoted_run_names_the_mode_its_signal_suggested() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("quick")),
        (1, Some("build"), started(1)),
        (2, Some("build"), finished()),
        (
            3,
            None,
            EventPayload::PromotionSignaled(PromotionSignaledPayload {
                reason: "re-routes exhausted".to_string(),
                evidence: "criteria still red".to_string(),
                suggested_mode: "full".into(),
            }),
        ),
        (4, None, run_finished(TerminalState::Promoted)),
    ]);

    assert_eq!(
        frame(&workflow, &events, 100).phase,
        RunPhase::Promoted {
            to: Some("full".into()),
        }
    );
}

#[test]
fn a_cancelled_run_is_reported_as_cancelled_not_as_finished() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), finished()),
        (3, None, run_finished(TerminalState::Cancelled)),
    ]);

    assert_eq!(frame(&workflow, &events, 100).phase, RunPhase::Cancelled);
}

#[test]
fn a_run_with_no_start_yet_is_created_and_a_finished_one_says_so() {
    let workflow = chain();
    let created_only = log(vec![(0, None, created("standard"))]);
    assert_eq!(
        frame(&workflow, &created_only, 100).phase,
        RunPhase::Created
    );

    let done = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), finished()),
        (3, None, run_finished(TerminalState::Done)),
    ]);
    let frame = frame(&workflow, &done, 9_000);
    assert_eq!(frame.phase, RunPhase::Finished);
    assert_eq!(
        frame.elapsed,
        Some(Duration::from_secs(3)),
        "a finished run's clock stops at its last event, whenever it is read"
    );
}

#[test]
fn a_broken_log_reports_its_diagnostic_and_keeps_what_replay_derived() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("standard")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        // No `node_started` for `build`: the log stops making sense here.
        (3, Some("build"), finished()),
    ]);

    let frame = frame(&workflow, &events, 100);
    match &frame.phase {
        RunPhase::Broken { diagnostic } => {
            assert!(
                diagnostic.contains("build") && diagnostic.contains("node_started"),
                "the diagnostic names the event that stopped replay: {diagnostic}"
            );
        }
        other => panic!("a log that stops making sense is broken, not {other:?}"),
    }
    assert_eq!(
        buckets(&frame.flow),
        [1, 0, 0, 0, 2],
        "what replay derived before the break is still reported"
    );
    let plan = frame.nodes.first().expect("declared");
    assert!(matches!(
        plan.state,
        NodeStanding::Reached(NodeState::Finished { .. })
    ));
}

#[test]
fn a_run_closed_with_no_evidence_behind_it_reports_none_instead_of_inventing_one() {
    // The evidence for how a run closed is a separate event: the last
    // `node_failed` for a failure, `promotion_signaled` for the mode a
    // promotion aimed at. A log that closed without one says so.
    let workflow = chain();
    let failed_blind = log(vec![
        (0, None, created("standard")),
        (1, Some("build"), started(1)),
        (2, Some("build"), finished()),
        (3, None, run_finished(TerminalState::Failed)),
    ]);
    assert_eq!(
        frame(&workflow, &failed_blind, 100).phase,
        RunPhase::Failed { failure: None }
    );

    let promoted_blind = log(vec![
        (0, None, created("quick")),
        (1, Some("build"), started(1)),
        (2, Some("build"), finished()),
        (3, None, run_finished(TerminalState::Promoted)),
    ]);
    assert_eq!(
        frame(&workflow, &promoted_blind, 100).phase,
        RunPhase::Promoted { to: None },
        "only the successor's own `run_created` names the mode"
    );
}

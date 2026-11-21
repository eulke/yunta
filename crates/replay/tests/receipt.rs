//! The receipt of a log written by hand: what it certifies comes off the
//! ledgers the log folds into, so a verdict the log superseded is never
//! counted beside the one that replaced it.

use std::collections::BTreeMap;

use yunta_core::events::{
    CriteriaCheckedPayload, Criterion, CriterionResult, EventPayload, NodeEvent,
    NodeFinishedPayload, NodeOutcome, NodeStartedPayload, Phase, RunCreatedPayload, RunEvent,
    RunFinishedPayload, RunMetrics, TaskEvent, TaskRegisteredPayload, TaskStatus,
    TaskStatusChangedPayload, TerminalState, TokenUsage,
};
use yunta_core::receipt::{CriteriaSummary, CriterionEntry, EventChainStatus};
use yunta_core::{CommitSha, ConfigLayer, ContentHash, Isolation, Manifest};
use yunta_replay::receipt;
use yunta_testkit_core::Log;

const RUN: &str = "run-receipt";

/// The one criterion `T001` is proved by.
const SUITE: &str = "cargo test";

/// A plan that registers tasks and a loop that works them, the two nodes
/// the log below names.
const WORKFLOW: &str = r#"
name: retried
nodes:
  - { id: plan, kind: prompt, prompt: "Write the tasks." }
  - id: implement
    kind: loop
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// A run frozen with [`WORKFLOW`].
fn manifest() -> Manifest {
    Manifest {
        schema_version: 1,
        yunta_version: "0.0.0".to_string(),
        workflow: yunta_core::yaml::parse(WORKFLOW).expect("a plan and a loop parse"),
        config: ConfigLayer::default(),
        inputs: BTreeMap::new(),
        prompts: BTreeMap::new(),
        base_branch: "main".to_string(),
        base_commit: CommitSha::from("abc1234"),
        isolation: Isolation::None,
        max_parallel_nodes: 1,
        workflow_hash: ContentHash::sha256(b"workflow"),
        config_hash: ContentHash::sha256(b"config"),
        paths: None,
        pack: None,
        project: None,
        adapter: None,
        frozen: None,
        provenance_gaps: Vec::new(),
        loaded_links: Vec::new(),
    }
}

fn created() -> EventPayload {
    EventPayload::Run(RunEvent::Created(RunCreatedPayload {
        checkout: None,
        manifest_hash: ContentHash::sha256(b"manifest"),
        inputs: BTreeMap::new(),
        mode: Default::default(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: CommitSha::from("abc1234"),
        environment: None,
        left_out: Vec::new(),
        opens_on_base: false,
    }))
}

fn started() -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1)))
}

fn finished() -> EventPayload {
    EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
        NodeOutcome::Summary("done".into()),
        TokenUsage::default(),
    )))
}

/// `T001`, proved by [`SUITE`] alone.
fn registered() -> EventPayload {
    EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
        task_id: "T001".into(),
        criteria: vec![Criterion {
            cmd: SUITE.to_string(),
            r#type: None,
        }],
        scope: Vec::new(),
        depends_on: Vec::new(),
    }))
}

/// `T001` at work, on the strength of the event at `caused_by`.
fn running(caused_by: u64) -> EventPayload {
    EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
        "T001".into(),
        TaskStatus::Running,
        caused_by.into(),
    )))
}

/// `T001` done, on the strength of the event at `caused_by`.
fn done(caused_by: u64) -> EventPayload {
    EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::done(
        "T001".into(),
        caused_by.into(),
        CommitSha::from("def5678"),
    )))
}

/// The run closed done, with its one task verified.
fn closed() -> EventPayload {
    EventPayload::Run(RunEvent::Finished(RunFinishedPayload::closed(
        TerminalState::Done,
        RunMetrics::of(TokenUsage::default(), 1),
    )))
}

/// A check of `T001` in `phase` where [`SUITE`] exited `exit_code`.
fn checked(phase: Phase, exit_code: i32) -> EventPayload {
    EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
        task_id: "T001".into(),
        phase,
        results: vec![CriterionResult {
            cmd: SUITE.to_string(),
            exit_code,
            r#type: None,
            reused: false,
            duration_ms: Some(900),
            output: None,
            tail: Vec::new(),
            tree: None,
            head: None,
            conclusion: None,
        }],
        waiting: Vec::new(),
    }))
}

#[test]
fn a_task_checked_red_after_its_work_then_green_is_counted_green_once() {
    let events = Log::for_run(RUN)
        .event(created())
        .node("plan", started())
        .node("plan", registered())
        .node("plan", finished())
        .node("implement", started())
        .node("implement", running(3))
        .node("implement", checked(Phase::Pre, 101))
        .after(60)
        .node("implement", checked(Phase::Post, 101))
        // The work is sent back for another attempt, which turns it green.
        .after(60)
        .node("implement", checked(Phase::Post, 0))
        .node("implement", done(9))
        .node("implement", finished())
        .event(closed())
        .build();
    let chain = EventChainStatus::Intact {
        events: events.len(),
    };

    let receipt = receipt::closed(&RUN.into(), &manifest(), &events, chain)
        .expect("a log read to its close certifies a closed run");

    assert_eq!(
        receipt.criteria,
        CriteriaSummary {
            total: 1,
            green: 1,
            tasks: 1,
            proving: 1,
            proved: 1,
            red_before: 1,
            guards: 0,
            guards_green: 0,
            entries: vec![CriterionEntry {
                task_id: "T001".to_string(),
                cmd: SUITE.to_string(),
                exit_code: 0,
            }],
        },
        "a retried task is certified by its last check after the work, and only by it, red \
         before the work as its check before it found"
    );
}

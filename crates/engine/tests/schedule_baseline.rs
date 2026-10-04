//! When [`decide`] has a run measure its lineage's suite: before any node
//! when the run measures in its own tree, and before only what reads the
//! measurement when it measures aside.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    BaselineCapturedPayload, BaselineOrigin, BaselineResults, EventBody, EventPayload, NodeEvent,
    NodeFinishedPayload, NodeStartedPayload, RunCreatedPayload, RunEvent, StoredEvent, TokenUsage,
};
use yunta_core::{CommitSha, ContentHash, DefaultOnFailure, NodeId, OnInterrupt, Workflow};
use yunta_engine::{decide, derive, Decision, SchedulingPolicy};

fn workflow() -> Workflow {
    yunta_core::yaml::parse(
        r#"
name: ship
nodes:
  - { id: plan, kind: bash, run: "true" }
  - { id: build, kind: bash, run: "true", depends_on: [plan] }
"#,
    )
    .expect("the test workflow parses")
}

fn policy() -> SchedulingPolicy {
    SchedulingPolicy {
        max_parallel_nodes: 2,
        on_interrupt: OnInterrupt::RestartNode,
        on_failure: DefaultOnFailure::Pause,
        mode_nodes: None,
        baseline_suite: None,
        measures_aside: false,
        may_promote: false,
        grants_scope: true,
        denied: Vec::new(),
    }
}

fn log(entries: Vec<(Option<&str>, EventPayload)>) -> Vec<StoredEvent> {
    let start: DateTime<Utc> = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    entries
        .into_iter()
        .enumerate()
        .map(|(index, (node, payload))| StoredEvent {
            run_id: "run-1".into(),
            seq: (index as u64 + 1).into(),
            timestamp: start + chrono::Duration::seconds(index as i64),
            node_id: node.map(Into::into),
            body: EventBody::Known(payload),
        })
        .collect()
}

fn created() -> EventPayload {
    EventPayload::Run(RunEvent::Created(RunCreatedPayload {
        checkout: None,
        manifest_hash: ContentHash::sha256(b"manifest"),
        inputs: BTreeMap::new(),
        mode: "default".into(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: CommitSha::from("abc1234"),
        environment: None,
        left_out: Vec::new(),
        opens_on_base: false,
    }))
}

/// The suite a run owes is a decision of the scheduler, like every other
/// thing a run does next: a pure read of what the log says the run holds.
/// Measured in the run's own tree, it comes before any node, since any
/// node may change that tree.
#[test]
fn a_run_owing_a_baseline_is_told_to_measure_it_before_any_node() {
    let events = log(vec![(None, created())]);
    let policy = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        ..policy()
    };

    assert_eq!(
        decide(&workflow(), &derive(&events), &policy),
        Decision::MeasureBaseline {
            suite: "cargo test".to_string()
        },
        "a run whose config names a suite and whose log holds no measurement owes one"
    );
}

/// Measured aside, the suite holds back only a step that reads it: the
/// node before is ready at once, and a node that compares against the
/// measurement waits for it.
#[test]
fn a_run_measuring_aside_waits_for_the_measurement_only_where_it_is_read() {
    let workflow: Workflow = yunta_core::yaml::parse(
        r#"
name: compare
nodes:
  - { id: plan, kind: bash, run: "true" }
  - { id: compare, kind: check, builtin: baseline_compare, depends_on: [plan] }
"#,
    )
    .expect("the test workflow parses");
    let policy = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        measures_aside: true,
        ..policy()
    };
    let fresh = log(vec![(None, created())]);
    let planned = log(vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "ok",
                TokenUsage::default(),
            ))),
        ),
    ]);

    assert_eq!(
        decide(&workflow, &derive(&fresh), &policy),
        Decision::Execute(vec![(NodeId::from("plan"), 1)]),
        "`plan` reads nothing the suite measures"
    );
    assert_eq!(
        decide(&workflow, &derive(&planned), &policy),
        Decision::MeasureBaseline {
            suite: "cargo test".to_string()
        },
        "`compare` reads the measurement, so it waits for it"
    );
}

/// A gate that puts a plan in front of a person shows the suite holding
/// each of its tasks, so it waits for the measurement as well; one that
/// shows no plan asks at once.
#[test]
fn a_gate_showing_a_plan_waits_for_the_measurement() {
    let gate = |shows: &str| -> Workflow {
        yunta_core::yaml::parse(&format!(
            "name: approve\nnodes:\n  - {{ id: plan, kind: bash, run: \"true\" }}\n  - {{ id: \
             approve, kind: gate, assignee: lead, depends_on: [plan]{shows} }}\n"
        ))
        .expect("the test workflow parses")
    };
    let aside = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        measures_aside: true,
        ..policy()
    };
    let planned = derive(&log(vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "ok",
                TokenUsage::default(),
            ))),
        ),
    ]));

    assert_eq!(
        decide(
            &gate(", shows: [{ node: plan, kind: tasks }]"),
            &planned,
            &aside
        ),
        Decision::MeasureBaseline {
            suite: "cargo test".to_string()
        }
    );
    assert_eq!(
        decide(&gate(""), &planned, &aside),
        Decision::ResolveInternalGate {
            node: NodeId::from("approve")
        }
    );
}

#[test]
fn a_run_born_holding_a_baseline_is_never_told_to_measure() {
    let held = EventPayload::Run(RunEvent::BaselineCaptured(BaselineCapturedPayload {
        command: "cargo test".to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: "ok".to_string(),
        },
        hash: yunta_core::sha256_hex(b"ok"),
        origin: BaselineOrigin::Inherited {
            run: "run-root".into(),
        },
        tree: None,
        duration_ms: None,
    }));
    let events = log(vec![(None, created()), (None, held)]);
    let policy = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        ..policy()
    };

    assert!(
        !matches!(
            decide(&workflow(), &derive(&events), &policy),
            Decision::MeasureBaseline { .. }
        ),
        "a run born holding its lineage's measurement owes nothing"
    );
}

/// A group runs its nodes together, so a group holding a node that reads
/// the measurement reads it: the comparison inside waits for it with the group.
#[test]
fn a_group_holding_a_reader_waits_for_the_measurement() {
    let workflow: Workflow = yunta_core::yaml::parse(
        r#"
name: grouped
nodes:
  - { id: plan, kind: bash, run: "true" }
  - id: both
    kind: parallel
    depends_on: [plan]
    nodes:
      - { id: lint, kind: bash, run: "true" }
      - { id: regressions, kind: check, builtin: baseline_compare }
"#,
    )
    .expect("the test workflow parses");
    let aside = SchedulingPolicy {
        baseline_suite: Some("cargo test".to_string()),
        measures_aside: true,
        ..policy()
    };
    let planned = derive(&log(vec![
        (None, created()),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        (
            Some("plan"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "ok",
                TokenUsage::default(),
            ))),
        ),
    ]));

    assert_eq!(
        decide(&workflow, &planned, &aside),
        Decision::MeasureBaseline {
            suite: "cargo test".to_string()
        }
    );
}

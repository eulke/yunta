//! An invariant's pass holds for the tree it left: [`decide`] runs it
//! again, before anything else starts, a gate is asked or the run
//! finishes, once a later node leaves the run at another tree.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    EventBody, EventPayload, Failure, NodeEvent, NodeFailedPayload, NodeFinishedPayload,
    NodeStartedPayload, RunCreatedPayload, RunEvent, StoredEvent, TokenUsage,
};
use yunta_core::{CommitSha, ContentHash, DefaultOnFailure, OnInterrupt, TreeId, Workflow};
use yunta_engine::{decide, derive, Decision, SchedulingPolicy};

const T1: &str = "a1a1a1a";
const T2: &str = "b2b2b2b";

/// Work, then the two checks of the tree, then a node after them that
/// may change it, then a gate.
const CHECKED: &str = r#"
name: checked
nodes:
  - { id: work, kind: bash, run: "true" }
  - { id: lint, kind: bash, run: "true", invariant: true, depends_on: [work] }
  - { id: tests, kind: check, builtin: baseline_compare, invariant: true, depends_on: [lint] }
  - { id: fix, kind: bash, run: "true", depends_on: [tests] }
  - { id: ship, kind: gate, assignee: lead, message: "Ship?", depends_on: [fix] }
"#;

fn workflow(yaml: &str) -> Workflow {
    yunta_core::yaml::parse(yaml).expect("the test workflow parses")
}

fn policy() -> SchedulingPolicy {
    SchedulingPolicy {
        max_parallel_nodes: 1,
        on_interrupt: OnInterrupt::RestartNode,
        on_failure: DefaultOnFailure::Pause,
        mode_nodes: None,
        baseline_suite: None,
        grants_scope: true,
    }
}

fn at(offset_secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::seconds(offset_secs)
}

fn tree(id: &str) -> TreeId {
    id.parse().unwrap()
}

/// One node's attempt, from the tree it started on to the tree it left.
fn ran(
    node: &'static str,
    attempt: u32,
    from: &str,
    left: &str,
) -> Vec<(&'static str, EventPayload)> {
    vec![
        (node, started(attempt, from)),
        (
            node,
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::leaving(
                "ok",
                TokenUsage::default(),
                tree(left),
            ))),
        ),
    ]
}

fn started(attempt: u32, from: &str) -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt_from(
        attempt,
        tree(from),
    )))
}

fn failed() -> EventPayload {
    EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::message("exit 1"),
        false,
        TokenUsage::default(),
    )))
}

/// What the run does next after `attempts`, in order, under `yaml`.
fn next(yaml: &str, attempts: Vec<Vec<(&'static str, EventPayload)>>) -> Decision {
    let created = EventPayload::Run(RunEvent::Created(RunCreatedPayload {
        manifest_hash: ContentHash::sha256(b"manifest"),
        inputs: BTreeMap::new(),
        mode: "default".into(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: CommitSha::from("abc1234"),
        environment: None,
        left_out: Vec::new(),
    }));
    let events: Vec<StoredEvent> = std::iter::once((None, created))
        .chain(
            attempts
                .into_iter()
                .flatten()
                .map(|(node, payload)| (Some(node), payload)),
        )
        .enumerate()
        .map(|(index, (node, payload))| StoredEvent {
            run_id: "run-1".into(),
            seq: (index as u64 + 1).into(),
            timestamp: at(index as i64),
            node_id: node.map(Into::into),
            body: EventBody::Known(payload),
        })
        .collect();
    decide(&workflow(yaml), &derive(&events), &policy())
}

fn runs(node: &str, attempt: u32) -> Decision {
    Decision::Execute(vec![(node.into(), attempt)])
}

fn asks(gate: &str) -> Decision {
    Decision::ResolveInternalGate { node: gate.into() }
}

#[test]
fn a_finished_invariant_runs_again_once_a_later_node_left_another_tree() {
    let checked = vec![
        ran("work", 1, T1, T1),
        ran("lint", 1, T1, T1),
        ran("tests", 1, T1, T1),
        ran("fix", 1, T1, T2),
    ];
    assert_eq!(
        next(CHECKED, checked),
        runs("lint", 2),
        "the first in declaration order"
    );
}

#[test]
fn an_invariant_is_not_run_again_while_the_tree_is_the_one_it_left() {
    let checked = vec![
        ran("work", 1, T1, T1),
        ran("lint", 1, T1, T1),
        ran("tests", 1, T1, T1),
        ran("fix", 1, T1, T1),
    ];
    assert_eq!(next(CHECKED, checked), asks("ship"));
}

#[test]
fn every_stale_invariant_runs_again_before_the_gate_is_asked() {
    let mut checked = vec![
        ran("work", 1, T1, T1),
        ran("lint", 1, T1, T1),
        ran("tests", 1, T1, T1),
        ran("fix", 1, T1, T2),
        ran("lint", 2, T2, T2),
    ];
    assert_eq!(next(CHECKED, checked.clone()), runs("tests", 2));
    checked.push(ran("tests", 2, T2, T2));
    assert_eq!(next(CHECKED, checked), asks("ship"));
}

#[test]
fn a_stale_invariant_runs_before_the_run_finishes() {
    let ungated = CHECKED.replace(
        "  - { id: ship, kind: gate, assignee: lead, message: \"Ship?\", depends_on: [fix] }\n",
        "",
    );
    let checked = vec![
        ran("work", 1, T1, T1),
        ran("lint", 1, T1, T1),
        ran("tests", 1, T1, T1),
        ran("fix", 1, T1, T2),
    ];
    assert_eq!(next(&ungated, checked), runs("lint", 2));
}

/// A person who changed the tree while the run was parked hands a node
/// back: the attempt starts from the new tree, and every check that
/// verified the tree before it runs again.
#[test]
fn a_retry_that_starts_from_a_changed_tree_makes_earlier_invariants_stale() {
    let checked = vec![
        ran("work", 1, T1, T1),
        ran("lint", 1, T1, T1),
        vec![("tests", started(1, T1)), ("tests", failed())],
        ran("tests", 2, T2, T2),
    ];
    assert_eq!(next(CHECKED, checked), runs("lint", 2));
}

#[test]
fn a_failure_is_resolved_before_any_invariant_runs_again() {
    let checked = vec![
        ran("work", 1, T1, T1),
        ran("lint", 1, T1, T1),
        vec![("tests", started(1, T2)), ("tests", failed())],
    ];
    assert!(
        matches!(next(CHECKED, checked), Decision::EscalateFailure { node, .. } if node.as_str() == "tests"),
    );
}

/// An invariant that rewrites the tree is not stale for its own change,
/// nor does its change make another stale: the next node that touches
/// the tree is what brings it into view.
#[test]
fn an_invariants_own_rewrite_makes_no_invariant_stale() {
    let formatted = r#"
name: formatted
nodes:
  - { id: lint, kind: bash, run: "true", invariant: true }
  - { id: fmt, kind: bash, run: "true", invariant: true, depends_on: [lint] }
  - { id: fix, kind: bash, run: "true", depends_on: [fmt] }
"#;
    let mut checked = vec![ran("lint", 1, T1, T1), ran("fmt", 1, T1, T2)];
    assert_eq!(next(formatted, checked.clone()), runs("fix", 1));
    checked.push(ran("fix", 1, T2, T2));
    assert_eq!(next(formatted, checked), runs("lint", 2));
}

#[test]
fn an_invariant_whose_finish_named_no_tree_is_never_run_again() {
    let unnamed = EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
        "ok",
        TokenUsage::default(),
    )));
    let checked = vec![
        ran("work", 1, T1, T1),
        vec![("lint", started(1, T1)), ("lint", unnamed)],
        ran("tests", 1, T1, T1),
        ran("fix", 1, T1, T1),
    ];
    let later = [checked, vec![ran("fix", 2, T1, T2)]].concat();
    assert_eq!(
        next(CHECKED, later),
        runs("tests", 2),
        "lint's pass names no tree"
    );
}

/// Only a pass that is a verdict on the tree is measured against it: a
/// check that reads the log, and a loop, never run again for it.
#[test]
fn a_node_whose_pass_is_no_verdict_on_the_tree_is_never_run_again() {
    let others = r#"
name: others
nodes:
  - { id: gate, kind: check, builtin: findings_gate, max_severity: blocking, invariant: true }
  - { id: implement, kind: loop, until: all_tasks_complete, prompt: "x", invariant: true, depends_on: [gate] }
  - { id: fix, kind: bash, run: "true", depends_on: [implement] }
"#;
    let checked = vec![
        ran("gate", 1, T1, T1),
        ran("implement", 1, T1, T1),
        ran("fix", 1, T1, T2),
    ];
    assert_eq!(next(others, checked), Decision::Finish);
}

// --- a tree that moves while an invariant runs ------------------------------

fn finish(left: &str) -> EventPayload {
    EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::leaving(
        "ok",
        TokenUsage::default(),
        tree(left),
    )))
}

/// `lint` runs beside `fix`, which lands another tree before `lint`
/// closes: whatever `lint` read, it may have read half-changed.
#[test]
fn an_invariant_that_ran_while_a_writer_finished_runs_again() {
    let beside = vec![
        ran("work", 1, T1, T1),
        vec![
            ("lint", started(1, T1)),
            ("fix", started(1, T1)),
            ("fix", finish(T2)),
            ("lint", finish(T2)),
        ],
    ];
    assert_eq!(next(CHECKED, beside), runs("lint", 2));
}

#[test]
fn an_invariant_that_finished_before_an_overlapping_writer_runs_again() {
    let beside = vec![
        ran("work", 1, T1, T1),
        vec![
            ("lint", started(1, T1)),
            ("fix", started(1, T1)),
            ("lint", finish(T2)),
            ("fix", finish(T2)),
        ],
    ];
    assert_eq!(next(CHECKED, beside), runs("lint", 2));
}

#[test]
fn an_invariant_beside_a_node_that_changed_nothing_is_not_run_again() {
    let beside = vec![
        ran("work", 1, T1, T1),
        vec![
            ("lint", started(1, T1)),
            ("fix", started(1, T1)),
            ("fix", finish(T1)),
            ("lint", finish(T1)),
        ],
    ];
    assert_eq!(next(CHECKED, beside), asks("ship"));
}

#[test]
fn an_invariant_that_started_from_the_tree_the_writer_left_is_not_run_again() {
    let beside = vec![
        ran("work", 1, T1, T1),
        vec![
            ("fix", started(1, T1)),
            ("lint", started(1, T2)),
            ("fix", finish(T2)),
            ("lint", finish(T2)),
        ],
    ];
    assert_eq!(next(CHECKED, beside), asks("ship"));
}

/// Run again alone, the invariant marks nobody and nobody marks it.
#[test]
fn an_invariant_run_again_alone_is_not_sent_round() {
    let beside = vec![
        ran("work", 1, T1, T1),
        vec![
            ("lint", started(1, T1)),
            ("fix", started(1, T1)),
            ("fix", finish(T2)),
            ("lint", finish(T2)),
        ],
        ran("lint", 2, T2, T2),
    ];
    assert_eq!(next(CHECKED, beside), asks("ship"));
}

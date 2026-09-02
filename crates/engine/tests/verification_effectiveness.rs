//! `analyze_verification_effectiveness` — golden
//! tests over hand-built historical logs, same style `tests/stats.rs`
//! uses for its own pure derivation.

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, EventBody, EventPayload, GateResolvedPayload,
    NodeFailedPayload, NodeReroutedPayload, Phase, StoredEvent,
};
use yunta_core::{Node, NodeKind, OnFailure, Workflow};
use yunta_engine::{analyze_verification_effectiveness as analyze, VERIFICATION_MIN_SAMPLES};

fn node(id: &str, on_failure: Option<OnFailure>) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Bash {
            run: "true".to_string(),
        },
        depends_on: Vec::new(),
        scope: Vec::new(),
        runner: None,
        artifacts: None,
        hooks: None,
        on_failure,
        on_interrupt: None,
        description: None,
        permissions: None,
        network: false,
        context: Vec::new(),
        invariant: false,
        skills: Vec::new(),
        interactive: false,
        runners: Vec::new(),
        agent: None,
    }
}

fn gate_node(id: &str) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Gate {
            assignee: "reviewer".to_string(),
            message: None,
            options: Vec::new(),
            on: Default::default(),
            external: Some(yunta_core::ExternalGate {
                kind: yunta_core::ForgeKind::PullRequest,
                artifacts: Vec::new(),
                branch: "{{run.branch}}".to_string(),
            }),
        },
        depends_on: Vec::new(),
        scope: Vec::new(),
        runner: None,
        artifacts: None,
        hooks: None,
        on_failure: None,
        on_interrupt: None,
        description: None,
        permissions: None,
        network: false,
        context: Vec::new(),
        invariant: false,
        skills: Vec::new(),
        interactive: false,
        runners: Vec::new(),
        agent: None,
    }
}

fn workflow(nodes: Vec<Node>) -> Workflow {
    Workflow {
        name: "fixture".into(),
        modes: None,
        description: None,
        inputs: Default::default(),
        node_defaults: None,
        nodes,
        yunta_schema: None,
        on_finish: Vec::new(),
    }
}

fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
}

/// `index` is the event's 0-based position in the synthetic log; storage
/// numbers positions from 1.
fn event(index: u64, node_id: Option<&str>, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        run_id: "run-1".into(),
        seq: (index + 1).into(),
        timestamp: base_time(),
        node_id: node_id.map(Into::into),
        body: EventBody::Known(payload),
    }
}

fn criterion(cmd: &str, exit_code: i32) -> CriterionResult {
    CriterionResult {
        cmd: cmd.to_string(),
        exit_code,
        r#type: None,
        reused: false,
        duration_ms: None,
    }
}

fn pre_check_run(cmd: &str, exit_code: i32) -> Vec<StoredEvent> {
    vec![event(
        0,
        None,
        EventPayload::CriteriaChecked(CriteriaCheckedPayload {
            task_id: "T001".into(),
            phase: Phase::Pre,
            results: vec![criterion(cmd, exit_code)],
        }),
    )]
}

#[test]
fn a_criterion_never_red_across_enough_samples_is_flagged() {
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|_| pre_check_run("cargo test", 0))
        .collect();
    let findings = analyze(&workflow(vec![]), &history);
    assert_eq!(findings.never_red_criteria.len(), 1);
    assert_eq!(findings.never_red_criteria[0].cmd, "cargo test");
    assert_eq!(
        findings.never_red_criteria[0].sample_count,
        VERIFICATION_MIN_SAMPLES
    );
}

#[test]
fn fewer_than_min_samples_flags_nothing() {
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES - 1)
        .map(|_| pre_check_run("cargo test", 0))
        .collect();
    let findings = analyze(&workflow(vec![]), &history);
    assert!(findings.never_red_criteria.is_empty());
}

#[test]
fn a_criterion_red_before_green_after_is_never_flagged() {
    // The relevant distinction is "never red before" vs.
    // "never failed" — this criterion *did* go red at least once, in
    // pre-check, which is the metric that matters; that it later passed
    // (post-check) is irrelevant to this signal.
    let mut history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|_| pre_check_run("cargo test", 0))
        .collect();
    history.push(pre_check_run("cargo test", 1)); // one red pre-check
    let findings = analyze(&workflow(vec![]), &history);
    assert!(
        findings.never_red_criteria.is_empty(),
        "a criterion that was red at least once in pre-check must never appear here"
    );
}

#[test]
fn a_reroute_that_never_fires_across_enough_failures_is_flagged() {
    let on_failure = OnFailure {
        goto: "fix".into(),
        max_reroutes: 2,
    };
    let wf = workflow(vec![node("lint", Some(on_failure))]);
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|i| {
            vec![event(
                i as u64,
                Some("lint"),
                EventPayload::NodeFailed(NodeFailedPayload {
                    outcome: "lint failed".to_string(),
                    tokens_used: Default::default(),
                    retryable: true,
                }),
            )]
            // no node_rerouted in any of these — the re-route this node
            // declares was never observed firing.
        })
        .collect();
    let findings = analyze(&wf, &history);
    assert_eq!(findings.never_triggered_reroutes.len(), 1);
    assert_eq!(findings.never_triggered_reroutes[0].node, "lint");
}

#[test]
fn a_reroute_that_fires_at_least_once_is_never_flagged() {
    let on_failure = OnFailure {
        goto: "fix".into(),
        max_reroutes: 2,
    };
    let wf = workflow(vec![node("lint", Some(on_failure))]);
    let mut history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|i| {
            vec![event(
                i as u64,
                Some("lint"),
                EventPayload::NodeFailed(NodeFailedPayload {
                    outcome: "lint failed".to_string(),
                    tokens_used: Default::default(),
                    retryable: true,
                }),
            )]
        })
        .collect();
    // The last sample's failure actually rerouted.
    history.push(vec![
        event(
            100,
            Some("lint"),
            EventPayload::NodeFailed(NodeFailedPayload {
                outcome: "lint failed".to_string(),
                tokens_used: Default::default(),
                retryable: true,
            }),
        ),
        event(
            101,
            Some("lint"),
            EventPayload::NodeRerouted(NodeReroutedPayload {
                to_node: "fix".into(),
                cause: "lint failed".to_string(),
                attempt: 1,
                max_reroutes: 2,
            }),
        ),
    ]);
    let findings = analyze(&wf, &history);
    assert!(findings.never_triggered_reroutes.is_empty());
}

#[test]
fn a_node_that_always_finishes_clean_is_flagged_even_though_it_never_failed() {
    // The strongest version of "this re-route never fired": the node
    // ran every time and never even needed it.
    use yunta_core::events::NodeFinishedPayload;
    let on_failure = OnFailure {
        goto: "fix".into(),
        max_reroutes: 2,
    };
    let wf = workflow(vec![node("lint", Some(on_failure))]);
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|i| {
            vec![event(
                i as u64,
                Some("lint"),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome: "clean".to_string(),
                    tokens_used: Default::default(),
                }),
            )]
        })
        .collect();
    let findings = analyze(&wf, &history);
    assert_eq!(findings.never_triggered_reroutes.len(), 1);
    assert_eq!(
        findings.never_triggered_reroutes[0].sample_count,
        VERIFICATION_MIN_SAMPLES
    );
}

#[test]
fn a_gate_always_approved_without_adjustment_is_flagged() {
    let wf = workflow(vec![gate_node("approve")]);
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|i| {
            vec![event(
                i as u64,
                Some("approve"),
                EventPayload::GateResolved(GateResolvedPayload {
                    chosen_option: None,
                    resolved_by: Some("reviewer".to_string()),
                    free_text: None,
                    approved_sha: Some("deadbeef".to_string()),
                }),
            )]
        })
        .collect();
    let findings = analyze(&wf, &history);
    assert_eq!(findings.always_approved_gates.len(), 1);
    assert_eq!(findings.always_approved_gates[0].node, "approve");
}

#[test]
fn a_gate_that_ever_needed_adjustment_is_never_flagged() {
    let wf = workflow(vec![gate_node("approve")]);
    let mut history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|i| {
            vec![event(
                i as u64,
                Some("approve"),
                EventPayload::GateResolved(GateResolvedPayload {
                    chosen_option: None,
                    resolved_by: Some("reviewer".to_string()),
                    free_text: None,
                    approved_sha: Some("deadbeef".to_string()),
                }),
            )]
        })
        .collect();
    history.push(vec![event(
        200,
        Some("approve"),
        EventPayload::GateResolved(GateResolvedPayload {
            chosen_option: None,
            resolved_by: Some("reviewer".to_string()),
            free_text: None,
            approved_sha: None, // changes requested / closed / retry
        }),
    )]);
    let findings = analyze(&wf, &history);
    assert!(findings.always_approved_gates.is_empty());
}

fn post_check_run(task_attempts: &[u32]) -> Vec<StoredEvent> {
    task_attempts
        .iter()
        .enumerate()
        .map(|(i, &attempts)| {
            let mut events = Vec::new();
            for a in 0..attempts {
                events.push(event(
                    (i as u64) * 10 + a as u64,
                    None,
                    EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                        task_id: format!("T{i:03}").parse().unwrap(),
                        phase: Phase::Post,
                        results: vec![criterion("test -f done", 0)],
                    }),
                ));
            }
            events
        })
        .fold(Vec::new(), |mut acc, mut v| {
            acc.append(&mut v);
            acc
        })
}

#[test]
fn tasks_always_passing_on_the_first_try_is_flagged() {
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|_| post_check_run(&[1, 1]))
        .collect();
    let findings = analyze(&workflow(vec![]), &history);
    assert!(findings.always_first_try_tasks.is_some());
}

#[test]
fn a_task_that_ever_needed_a_retry_is_never_flagged() {
    let mut history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|_| post_check_run(&[1, 1]))
        .collect();
    history.push(post_check_run(&[2]));
    let findings = analyze(&workflow(vec![]), &history);
    assert!(findings.always_first_try_tasks.is_none());
}

#[test]
fn no_history_flags_nothing_at_all() {
    let findings = analyze(&workflow(vec![node("x", None)]), &[]);
    assert!(findings.is_empty());
}

// --- signals tied to modes -----------

fn run_created_in_mode(mode: &str) -> Vec<StoredEvent> {
    vec![event(
        0,
        None,
        EventPayload::RunCreated(yunta_core::events::RunCreatedPayload {
            manifest_hash: "h".to_string(),
            inputs: std::collections::BTreeMap::new(),
            mode: mode.into(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "abc".to_string(),
        }),
    )]
}

fn moded_workflow(nodes: Vec<Node>, mode_names: &[&str]) -> Workflow {
    let mut wf = workflow(nodes);
    wf.modes = Some(
        mode_names
            .iter()
            .map(|name| {
                (
                    yunta_core::ModeName::from(*name),
                    yunta_core::ModeSpec {
                        include: yunta_core::ModeInclude::All,
                    },
                )
            })
            .collect(),
    );
    wf
}

#[test]
fn a_declared_mode_never_used_across_enough_runs_is_flagged() {
    let wf = moded_workflow(vec![], &["quick", "standard", "full"]);
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|_| run_created_in_mode("quick"))
        .collect();
    let findings = analyze(&wf, &history);
    let unused: Vec<&str> = findings
        .unused_modes
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(unused, vec!["standard", "full"]);
    assert_eq!(
        findings.unused_modes[0].runs_observed,
        VERIFICATION_MIN_SAMPLES
    );
}

#[test]
fn with_fewer_runs_than_the_floor_no_mode_is_flagged() {
    let wf = moded_workflow(vec![], &["quick", "full"]);
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES - 1)
        .map(|_| run_created_in_mode("quick"))
        .collect();
    let findings = analyze(&wf, &history);
    assert!(findings.unused_modes.is_empty());
}

#[test]
fn a_mode_used_even_once_is_never_flagged() {
    let wf = moded_workflow(vec![], &["quick", "full"]);
    let mut history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|_| run_created_in_mode("quick"))
        .collect();
    history.push(run_created_in_mode("full"));
    let findings = analyze(&wf, &history);
    assert!(findings.unused_modes.is_empty());
}

#[test]
fn an_invariant_node_is_never_the_subject_of_a_remove_shaped_finding() {
    // Structural guarantee: never suggests removing `invariant:
    // true` nodes — a verification node that never fails is doing its job;
    // its never-fired re-route and its always-approved gate are
    // excluded from the findings by construction.
    use yunta_core::events::NodeFinishedPayload;
    let mut lint = node(
        "lint",
        Some(OnFailure {
            goto: "fix".into(),
            max_reroutes: 2,
        }),
    );
    lint.invariant = true;
    let wf = workflow(vec![lint]);
    let history: Vec<Vec<StoredEvent>> = (0..VERIFICATION_MIN_SAMPLES)
        .map(|i| {
            vec![event(
                i as u64,
                Some("lint"),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome: "clean".to_string(),
                    tokens_used: Default::default(),
                }),
            )]
        })
        .collect();
    let findings = analyze(&wf, &history);
    assert!(
        findings.never_triggered_reroutes.is_empty(),
        "an invariant node's never-fired re-route must never be flagged: {findings:?}"
    );
}

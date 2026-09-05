//! `compute_run_stats`/`prior_estimation` —
//! golden tests over a hand-built log fixture with a retry and a
//! re-route, exercised directly without spinning up a full run (same
//! style `tests/progress.rs` uses for `render_progress`).

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    EventBody, EventPayload, NodeFailedPayload, NodeFinishedPayload, NodeReroutedPayload,
    NodeStartedPayload, RunCreatedPayload, RunnerResolvedPayload, StoredEvent,
    TaskRegisteredPayload, TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::{Node, NodeKind, RunnerCandidate, Workflow};
use yunta_engine::{compute_run_stats, prior_estimation, run_summary, RunSummary};

fn node(id: &str, depends_on: &[&str]) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Bash {
            run: "true".to_string(),
        },
        depends_on: depends_on.iter().map(|d| (*d).into()).collect(),
        scope: Vec::new(),
        runner: Some("implementer".into()),
        artifacts: None,
        hooks: None,
        on_failure: None,
        on_interrupt: None,
        description: None,
        permissions: None,
        network: None,
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
fn event(
    index: u64,
    offset_secs: i64,
    node_id: Option<&str>,
    payload: EventPayload,
) -> StoredEvent {
    StoredEvent {
        run_id: "run-1".into(),
        seq: (index + 1).into(),
        timestamp: base_time() + chrono::Duration::seconds(offset_secs),
        node_id: node_id.map(Into::into),
        body: EventBody::Known(payload),
    }
}

fn candidate() -> RunnerCandidate {
    RunnerCandidate {
        adapter: "mock".into(),
        model: "mock-model".into(),
        agent: None,
    }
}

fn tokens(input: u64, output: u64, cached: Option<u64>) -> TokenUsage {
    TokenUsage {
        input,
        output,
        cached,
    }
}

/// Two nodes: `a` runs clean in one attempt; `b` depends on `a`, starts 5s
/// after `a` finishes (blocked time), fails on attempt 1, re-routes, and
/// finishes on attempt 2 — a retry's tokens count as rework, and the
/// second `RunnerResolved` re-affirms the same role.
fn fixture_events() -> Vec<StoredEvent> {
    vec![
        event(
            0,
            0,
            None,
            EventPayload::RunCreated(RunCreatedPayload {
                manifest_hash: yunta_core::sha256_hex(b"h"),
                inputs: Default::default(),
                mode: "default".into(),
                promoted_from: None,
                yunta_schema: None,
                base_branch: "main".to_string(),
                base_commit: "deadbeef".into(),
            }),
        ),
        event(
            1,
            0,
            Some("a"),
            EventPayload::RunnerResolved(RunnerResolvedPayload {
                runner: "implementer".into(),
                chosen: candidate(),
                discarded: Vec::new(),
            }),
        ),
        event(
            2,
            0,
            Some("a"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            3,
            10,
            Some("a"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "ok".to_string(),
                tokens_used: tokens(100, 50, Some(20)),
            }),
        ),
        event(
            4,
            10,
            Some("b"),
            EventPayload::RunnerResolved(RunnerResolvedPayload {
                runner: "implementer".into(),
                chosen: candidate(),
                discarded: Vec::new(),
            }),
        ),
        // `b` becomes ready at t=10s (when `a` finishes) but only starts
        // at t=15s — 5s blocked.
        event(
            5,
            15,
            Some("b"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            6,
            25,
            Some("b"),
            EventPayload::NodeFailed(NodeFailedPayload {
                outcome: "criteria still red".to_string(),
                tokens_used: tokens(80, 40, None),
                retryable: true,
            }),
        ),
        event(
            7,
            25,
            Some("b"),
            EventPayload::NodeRerouted(NodeReroutedPayload {
                to_node: "b".into(),
                cause: "criteria still red".to_string(),
                attempt: Some(2),
                max_reroutes: Some(1),
                origin: yunta_core::events::RerouteOrigin::OnFailure,
            }),
        ),
        event(
            8,
            26,
            Some("b"),
            EventPayload::RunnerResolved(RunnerResolvedPayload {
                runner: "implementer".into(),
                chosen: candidate(),
                discarded: Vec::new(),
            }),
        ),
        event(
            9,
            26,
            Some("b"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 2 }),
        ),
        event(
            10,
            36,
            Some("b"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "ok".to_string(),
                tokens_used: tokens(60, 30, None),
            }),
        ),
        event(
            11,
            36,
            None,
            EventPayload::TaskRegistered(TaskRegisteredPayload {
                task_id: "t1".into(),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            }),
        ),
        event(
            12,
            37,
            None,
            EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                task_id: "t1".into(),
                new_status: TaskStatus::Done,
                caused_by: 10.into(),
            }),
        ),
    ]
}

#[test]
fn cptv_is_total_tokens_over_tasks_done() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());

    // total tokens: a=150 (100+50), b=120+90=210 -> 360; one task done.
    assert_eq!(stats.cptv, Some(360.0));
    assert_eq!(stats.tasks_total, 1);
    assert_eq!(stats.tasks_done, 1);
}

#[test]
fn rework_rate_counts_only_tokens_spent_past_the_first_attempt() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());

    // b's second attempt spent 60+30=90 tokens out of 360 total.
    assert_eq!(stats.rework_rate, Some(90.0 / 360.0));
}

#[test]
fn cache_rate_is_none_unless_some_attempt_reported_it() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());

    // Only `a`'s attempt reported a cached figure (20), out of 240 input
    // tokens total (100 + 80 + 60).
    assert_eq!(stats.cache_rate, Some(20.0 / 240.0));

    let wf_solo = workflow(vec![node("a", &[])]);
    let no_cache_events: Vec<StoredEvent> = fixture_events()
        .into_iter()
        .take(4)
        .map(|mut e| {
            if let EventBody::Known(EventPayload::NodeFinished(p)) = &mut e.body {
                p.tokens_used.cached = None;
            }
            e
        })
        .collect();
    let solo_stats = compute_run_stats(&wf_solo, &no_cache_events);
    assert_eq!(solo_stats.cache_rate, None);
}

#[test]
fn cache_rate_is_none_without_input() {
    // A run can report a cache figure yet spend no input tokens to divide
    // by; a rate over zero input is undefined, never a fabricated 0%.
    let wf = workflow(vec![node("a", &[])]);
    let events = vec![
        event(
            0,
            0,
            None,
            EventPayload::RunCreated(RunCreatedPayload {
                manifest_hash: yunta_core::sha256_hex(b"h"),
                inputs: Default::default(),
                mode: "default".into(),
                promoted_from: None,
                yunta_schema: None,
                base_branch: "main".to_string(),
                base_commit: "deadbeef".into(),
            }),
        ),
        event(
            1,
            0,
            Some("a"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            5,
            Some("a"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "ok".to_string(),
                tokens_used: tokens(0, 50, Some(0)),
            }),
        ),
    ];
    let stats = compute_run_stats(&wf, &events);
    assert_eq!(stats.total_tokens.input, 0);
    assert_eq!(stats.cache_rate, None);
}

#[test]
fn median_of_even_count_is_the_mean_of_the_two_middles() {
    // Nearest-rank picks one side; a median of an even count averages the
    // two central samples, and a median of nothing is None, not zero.
    assert_eq!(yunta_engine::median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    assert_eq!(yunta_engine::median(&[10.0]), Some(10.0));
    assert_eq!(yunta_engine::median(&[1.0, 2.0, 3.0]), Some(2.0));
    assert_eq!(yunta_engine::median(&[]), None);
}

#[test]
fn a_retried_node_reports_its_max_attempt_and_summed_tokens() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());

    let b = stats
        .nodes
        .iter()
        .find(|n| n.node_id.as_str() == "b")
        .unwrap();
    assert_eq!(b.attempts, 2);
    assert_eq!(b.tokens.input, 140); // 80 + 60
    assert_eq!(b.tokens.output, 70); // 40 + 30
    assert_eq!(b.active, std::time::Duration::from_secs(10 + 10));
}

#[test]
fn a_node_that_starts_after_its_dependency_finishes_records_blocked_time() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());

    let a = stats
        .nodes
        .iter()
        .find(|n| n.node_id.as_str() == "a")
        .unwrap();
    assert_eq!(a.blocked, std::time::Duration::ZERO);
    assert_eq!(a.blocked_fraction(), Some(0.0));

    let b = stats
        .nodes
        .iter()
        .find(|n| n.node_id.as_str() == "b")
        .unwrap();
    // ready at t=10 (a finishes), started at t=15 -> 5s blocked.
    assert_eq!(b.blocked, std::time::Duration::from_secs(5));
    // wall clock = 5s blocked + 20s active = 25s -> 5/25 = 0.2.
    assert_eq!(b.blocked_fraction(), Some(0.2));
}

#[test]
fn tokens_by_role_groups_every_node_under_its_resolved_role() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());

    let by_role = stats.tokens_by_runner();
    assert_eq!(by_role.len(), 1);
    let (role, tokens) = &by_role[0];
    assert_eq!(role, "implementer");
    assert_eq!(tokens.input, 240); // 100 + 140
    assert_eq!(tokens.output, 120); // 50 + 70
}

#[test]
fn wall_clock_spans_the_first_to_the_last_event() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let stats = compute_run_stats(&wf, &fixture_events());
    assert_eq!(stats.wall_clock, Some(std::time::Duration::from_secs(37)));
}

#[test]
fn a_node_that_never_started_is_absent_from_the_node_list() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"]), node("c", &[])]);
    let stats = compute_run_stats(&wf, &fixture_events());
    assert!(!stats.nodes.iter().any(|n| n.node_id.as_str() == "c"));
}

fn summary(tokens: u64, wall_clock_secs: u64, tasks_total: usize) -> RunSummary {
    RunSummary {
        run_id: "r".into(),
        mode: "default".into(),
        workflow_hash: yunta_core::sha256_hex(b"h"),
        tokens,
        wall_clock: Some(std::time::Duration::from_secs(wall_clock_secs)),
        tasks_total,
        cptv: Some(tokens as f64 / tasks_total.max(1) as f64),
    }
}

#[test]
fn fewer_than_three_runs_means_no_estimation_at_all() {
    let history = vec![summary(100, 10, 1), summary(200, 20, 2)];
    assert_eq!(prior_estimation(&history), None);
}

#[test]
fn three_or_more_runs_produce_median_and_p90() {
    let history = vec![
        summary(100, 10, 1),
        summary(200, 20, 2),
        summary(300, 30, 3),
    ];
    let estimation = prior_estimation(&history).expect("3 samples must estimate");
    assert_eq!(estimation.sample_count, 3);
    assert_eq!(estimation.tokens.median, 200.0);
    assert_eq!(estimation.tokens.p90, 300.0);
    let wall_clock = estimation
        .wall_clock_secs
        .expect("every run reported a wall-clock");
    assert_eq!(wall_clock.median, 20.0);
    assert_eq!(estimation.tasks.median, 2.0);
}

#[test]
fn wall_clock_percentiles_are_absent_when_no_run_measured_one() {
    // A run without a measurable wall-clock contributes no fabricated
    // zero-second sample; with none measured, the estimate simply omits it
    // rather than reporting a manufactured 0s median.
    let history: Vec<RunSummary> = (0..3)
        .map(|i| RunSummary {
            run_id: "r".into(),
            mode: "default".into(),
            workflow_hash: yunta_core::sha256_hex(b"h"),
            tokens: 100 * (i + 1),
            wall_clock: None,
            tasks_total: (i + 1) as usize,
            cptv: None,
        })
        .collect();
    let estimation = prior_estimation(&history).expect("3 samples estimate tokens and tasks");
    assert_eq!(estimation.wall_clock_secs, None);
    assert_eq!(estimation.tokens.median, 200.0);
}

#[test]
fn run_summary_reuses_compute_run_stats_for_its_own_numbers() {
    let wf = workflow(vec![node("a", &[]), node("b", &["a"])]);
    let events = fixture_events();
    let summary = run_summary(
        "run-1".into(),
        "default".into(),
        yunta_core::sha256_hex(b"workflow-hash"),
        &wf,
        &events,
    );
    assert_eq!(summary.tokens, 360);
    assert_eq!(summary.cptv, Some(360.0));
    assert_eq!(summary.tasks_total, 1);
    assert_eq!(summary.wall_clock, Some(std::time::Duration::from_secs(37)));
}

// --- budget-vs-p90 warning -----------------------------

#[test]
fn a_cap_below_the_historical_p90_produces_the_warning() {
    let history = vec![
        summary(100, 10, 1),
        summary(200, 20, 2),
        summary(300, 30, 3),
    ];
    let estimation = prior_estimation(&history);
    let warning = yunta_engine::budget_p90_warning(Some(250), estimation.as_ref())
        .expect("cap 250 < p90 300 must warn");
    assert_eq!(
        warning,
        "warning: `limits.max_tokens_per_run` (250) is below this workflow's historical p90 (300 tokens over 3 run(s)) — the run may pause on its budget"
    );
}

#[test]
fn a_cap_at_or_above_the_p90_or_missing_pieces_stay_silent() {
    let history = vec![
        summary(100, 10, 1),
        summary(200, 20, 2),
        summary(300, 30, 3),
    ];
    let estimation = prior_estimation(&history);
    // Cap covers the p90: nothing to say.
    assert_eq!(
        yunta_engine::budget_p90_warning(Some(300), estimation.as_ref()),
        None
    );
    // No cap declared: nothing to compare.
    assert_eq!(
        yunta_engine::budget_p90_warning(None, estimation.as_ref()),
        None
    );
    // Not enough history (<3 runs): the estimation itself is None.
    assert_eq!(yunta_engine::budget_p90_warning(Some(1), None), None);
}

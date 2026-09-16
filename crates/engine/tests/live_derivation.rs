//! The derivations a view of a run *in progress* rests on: the elapsed
//! time of an attempt still open, a run clock that keeps moving while a
//! node says nothing, the node a ledger task belongs to, the sessions
//! and tool calls of the attempt now running, and the tokens spent so
//! far. Each is a pure function of the event log plus an instant the
//! caller supplies — the logs here are built by hand with fixed
//! timestamps, so no assertion depends on how long anything takes.

use std::time::Duration;

use chrono::{DateTime, Utc};
use proptest::prelude::*;
use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, AgentSessionOpenedPayload, Capabilities, EventPayload,
    Failure, NodeFailedPayload, NodeFinishedPayload, NodeStartedPayload, RunFinishedPayload,
    RunMetrics, TaskRegisteredPayload, TaskStatus, TaskStatusChangedPayload, TerminalState,
    TokenUsage,
};
use yunta_core::events::{NodeEvent, RunEvent, SessionEvent, TaskEvent};
use yunta_core::{Node, NodeKind, Workflow};
use yunta_engine::{
    compute_run_stats, compute_run_stats_at, derive, last_event_age, live_total_tokens,
    open_sessions, recent_tool_calls, running_since,
};
use yunta_testkit_core::{fixed_now, Log};

/// The instant `offset_secs` from the origin every log here starts at.
fn at(offset_secs: i64) -> DateTime<Utc> {
    fixed_now() + chrono::Duration::seconds(offset_secs)
}

fn node(id: &str) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Bash {
            run: "true".to_string(),
        },
        depends_on: Vec::new(),
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
        runners: Vec::new(),
        agent: None,
    }
}

fn workflow(ids: &[&str]) -> Workflow {
    Workflow {
        name: "fixture".into(),
        modes: None,
        description: None,
        inputs: Default::default(),
        node_defaults: None,
        nodes: ids.iter().map(|id| node(id)).collect(),
        yunta_schema: None,
        on_finish: Vec::new(),
    }
}

fn tokens(input: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        output,
        cached: None,
    }
}

fn started(attempt: u32) -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(attempt)))
}

fn finished(tokens_used: TokenUsage) -> EventPayload {
    EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload {
        outcome: "ok".to_string(),
        tokens_used,
    }))
}

fn failed(tokens_used: TokenUsage) -> EventPayload {
    EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::message("criteria still red".to_string()),
        true,
        tokens_used,
    )))
}

fn usage(input: u64, output: u64) -> EventPayload {
    EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
        message_type: AgentMessageType::Usage,
        tool_name: None,
        target: None,
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        text: None,
    }))
}

fn tool_use(tool: &str, digest: &str) -> EventPayload {
    EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
        message_type: AgentMessageType::ToolUse,
        tool_name: Some(tool.to_string()),
        target: Some(yunta_core::events::ToolTarget::opaque(digest.as_bytes())),
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        text: None,
    }))
}

fn session_opened(session: &str) -> EventPayload {
    EventPayload::Session(SessionEvent::Opened(AgentSessionOpenedPayload {
        session_id: session.into(),
        agent: Some("reviewer".into()),
        model: Some("mock-model".into()),
        capabilities: Capabilities::default(),
        fence: None,
    }))
}

/// One node, one attempt, still open: a session, two tool calls and one
/// usage report, the last of them at t=30s. The builder comes back with
/// its clock there, so a test states what follows on top of it.
fn in_progress() -> Log {
    Log::for_run("run-1")
        .node("build", started(1))
        .after(5)
        .node("build", session_opened("s-1"))
        .after(5)
        .node("build", tool_use("rg", "digest-rg"))
        .after(10)
        .node("build", usage(100, 50))
        .after(10)
        .node("build", tool_use("cargo", "digest-cargo"))
}

// --- the elapsed time of an open attempt -------------------------------

#[test]
fn a_node_on_its_first_attempt_reports_how_long_that_attempt_has_run() {
    let stats = compute_run_stats_at(&workflow(&["build"]), &in_progress().build(), at(90));
    let build = &stats.nodes[0];

    // Nothing closed yet, so the sum over closed attempts is zero — the
    // open attempt is what the node has actually been running.
    assert_eq!(build.active, Duration::ZERO);
    assert_eq!(build.open_attempt, Some(Duration::from_secs(90)));
    assert_eq!(build.active_so_far(), Duration::from_secs(90));
    assert_eq!(build.wall_clock(), Duration::from_secs(90));
}

#[test]
fn a_closed_attempt_leaves_no_open_elapsed_behind() {
    let events = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)))
        .build();

    let stats = compute_run_stats_at(&workflow(&["build"]), &events, at(500));
    let build = &stats.nodes[0];
    assert_eq!(build.open_attempt, None);
    assert_eq!(build.active, Duration::from_secs(100));
    assert_eq!(build.active_so_far(), Duration::from_secs(100));
}

#[test]
fn a_second_attempt_reports_its_own_elapsed_on_top_of_the_first() {
    let events = Log::for_run("run-1")
        .node("build", started(1))
        .after(10)
        .node("build", failed(tokens(10, 5)))
        .after(10)
        .node("build", started(2))
        .build();

    let stats = compute_run_stats_at(&workflow(&["build"]), &events, at(35));
    let build = &stats.nodes[0];
    assert_eq!(build.active, Duration::from_secs(10));
    assert_eq!(build.open_attempt, Some(Duration::from_secs(15)));
    assert_eq!(build.active_so_far(), Duration::from_secs(25));
}

#[test]
fn stats_without_an_instant_report_no_open_elapsed_at_all() {
    // `compute_run_stats` observes a run as of its own last event and has
    // no later instant to measure an open attempt against: it reports the
    // attempt's elapsed as unknown, never as zero seconds.
    let stats = compute_run_stats(&workflow(&["build"]), &in_progress().build());
    let build = &stats.nodes[0];
    assert_eq!(build.open_attempt, None);
    assert_eq!(build.active, Duration::ZERO);
    assert_eq!(stats.wall_clock, Some(Duration::from_secs(30)));
}

#[test]
fn an_open_attempts_elapsed_is_the_time_since_the_start_the_log_names() {
    // The elapsed a node stat reports and the start `running_since`
    // finds are the same fact read two ways; a view that mixes them
    // shows one attempt, not two.
    let events = in_progress().build();
    let now = at(90);
    let started = running_since(&derive(&events), &"build".into()).expect("the attempt is open");

    let stats = compute_run_stats_at(&workflow(&["build"]), &events, now);
    assert_eq!(
        stats.nodes[0].open_attempt,
        Some((now - started).to_std().expect("now is after the start"))
    );
}

// --- the run clock -----------------------------------------------------

#[test]
fn a_run_still_going_measures_its_wall_clock_to_now() {
    // The last event is at t=30s; a node that has been silent for a
    // minute must not freeze the run's clock there.
    let stats = compute_run_stats_at(&workflow(&["build"]), &in_progress().build(), at(90));
    assert_eq!(stats.wall_clock, Some(Duration::from_secs(90)));
}

#[test]
fn a_finished_run_stops_its_clock_at_its_last_event() {
    let events = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)))
        .after(10)
        .event(EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
            terminal_state: TerminalState::Done,
            metrics: RunMetrics {
                cptv: None,
                tokens: tokens(100, 50),
            },
        })))
        .build();

    let stats = compute_run_stats_at(&workflow(&["build"]), &events, at(9_000));
    assert_eq!(stats.wall_clock, Some(Duration::from_secs(110)));
}

#[test]
fn a_run_read_at_an_instant_before_its_first_event_has_no_negative_clock() {
    // A caller's clock behind the log still gets a measured answer:
    // nothing has elapsed yet, which is not the same as an empty log.
    let stats = compute_run_stats_at(&workflow(&["build"]), &in_progress().build(), at(-60));
    assert_eq!(stats.wall_clock, Some(Duration::ZERO));
    assert_eq!(stats.nodes[0].open_attempt, Some(Duration::ZERO));
}

// --- the node a task belongs to ----------------------------------------

#[test]
fn each_loop_nodes_tasks_keep_the_node_that_registered_them() {
    let events = Log::for_run("run-1")
        .node("review-a", started(1))
        .node("review-b", started(1))
        .after(1)
        .node(
            "review-a",
            EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                task_id: "t1".into(),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            })),
        )
        .node(
            "review-b",
            EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                task_id: "t2".into(),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            })),
        )
        .after(1)
        .node(
            "review-b",
            EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
                "t2".into(),
                TaskStatus::Running,
                4.into(),
            ))),
        )
        .build();

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(
        state.tasks.get("t1").and_then(|r| r.owner.as_ref()),
        Some(&"review-a".into())
    );
    assert_eq!(
        state.tasks.get("t2").and_then(|r| r.owner.as_ref()),
        Some(&"review-b".into())
    );
}

#[test]
fn a_task_no_event_attributes_to_a_node_has_no_owner() {
    let events = Log::for_run("run-1")
        .event(EventPayload::Tasks(TaskEvent::Registered(
            TaskRegisteredPayload {
                task_id: "t1".into(),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            },
        )))
        .build();

    let state = derive(&events);
    assert_eq!(state.tasks.status("t1"), Some(TaskStatus::Pending));
    assert_eq!(state.tasks.get("t1").and_then(|r| r.owner.as_ref()), None);
}

// --- what a node is doing now ------------------------------------------

#[test]
fn running_since_is_the_last_start_with_no_terminal_after_it() {
    assert_eq!(
        running_since(&derive(&in_progress().build()), &"build".into()),
        Some(at(0))
    );

    let closed = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)));
    assert_eq!(
        running_since(&derive(&closed.build()), &"build".into()),
        None
    );

    let restarted = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)))
        .at(at(120))
        .node("build", started(2))
        .build();
    assert_eq!(
        running_since(&derive(&restarted), &"build".into()),
        Some(at(120))
    );
}

#[test]
fn a_node_that_never_started_is_running_since_nothing() {
    assert_eq!(
        running_since(&derive(&in_progress().build()), &"absent".into()),
        None
    );
}

#[test]
fn the_age_of_a_nodes_last_event_grows_with_the_instant_it_is_read_at() {
    let events = in_progress().build();
    assert_eq!(
        last_event_age(&derive(&events), &"build".into(), at(90)),
        Some(Duration::from_secs(60))
    );
    assert_eq!(
        last_event_age(&derive(&events), &"build".into(), at(630)),
        Some(Duration::from_secs(600))
    );
    assert_eq!(
        last_event_age(&derive(&events), &"absent".into(), at(90)),
        None
    );
}

#[test]
fn an_event_stamped_after_the_instant_it_is_read_at_has_no_negative_age() {
    assert_eq!(
        last_event_age(&derive(&in_progress().build()), &"build".into(), at(0)),
        Some(Duration::ZERO)
    );
}

#[test]
fn the_sessions_open_on_a_node_are_the_ones_opened_since_its_last_terminal() {
    let sessions = open_sessions(&derive(&in_progress().build()), &"build".into());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id.as_str(), "s-1");
    assert_eq!(sessions[0].agent, Some("reviewer".into()));
    assert_eq!(sessions[0].model, Some("mock-model".into()));
    assert_eq!(sessions[0].opened_at, at(5));

    let closed = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)));
    assert!(open_sessions(&derive(&closed.build()), &"build".into()).is_empty());

    let restarted = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)))
        .at(at(120))
        .node("build", started(2))
        .after(1)
        .node("build", session_opened("s-2"))
        .build();
    let reopened = open_sessions(&derive(&restarted), &"build".into());
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0].session_id.as_str(), "s-2");
}

#[test]
fn tool_calls_come_back_newest_first_and_capped_at_the_limit() {
    let events = in_progress().build();
    let calls = recent_tool_calls(&derive(&events), &"build".into(), 10);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].tool_name.as_deref(), Some("cargo"));
    assert_eq!(
        calls[0].target.as_ref().map(|target| target.digest.clone()),
        Some(yunta_core::sha256_hex(b"digest-cargo"))
    );
    assert_eq!(calls[0].at, at(30));
    assert_eq!(calls[1].tool_name.as_deref(), Some("rg"));

    let one = recent_tool_calls(&derive(&events), &"build".into(), 1);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].tool_name.as_deref(), Some("cargo"));
}

#[test]
fn a_nodes_tool_calls_are_its_own_and_stop_at_its_last_terminal() {
    let elsewhere = in_progress()
        .after(1)
        .node("other", tool_use("sed", "digest-sed"));
    let events = elsewhere.build();
    assert_eq!(
        recent_tool_calls(&derive(&events), &"other".into(), 10).len(),
        1
    );
    assert_eq!(
        recent_tool_calls(&derive(&events), &"build".into(), 10).len(),
        2
    );

    let closed = in_progress()
        .after(1)
        .node("other", tool_use("sed", "digest-sed"))
        .at(at(100))
        .node("build", finished(tokens(100, 50)))
        .build();
    assert!(recent_tool_calls(&derive(&closed), &"build".into(), 10).is_empty());
}

// --- the tokens spent so far -------------------------------------------

#[test]
fn a_live_total_adds_the_usage_a_running_node_has_reported() {
    let events = in_progress().build();
    // Nothing has terminated, so replay's own total is still zero.
    assert_eq!(derive(&events).total_tokens(), TokenUsage::default());
    assert_eq!(live_total_tokens(&events), tokens(100, 50));
}

#[test]
fn a_terminated_nodes_usage_is_not_counted_on_top_of_its_terminal() {
    let events = in_progress()
        .at(at(100))
        .node("build", finished(tokens(100, 50)))
        .build();

    // The usage events and `tokens_used` describe the same 150 tokens.
    assert_eq!(derive(&events).total_tokens(), tokens(100, 50));
    assert_eq!(live_total_tokens(&events), tokens(100, 50));
}

#[test]
fn a_new_attempt_counts_only_its_own_usage_over_the_closed_one() {
    let events = Log::for_run("run-1")
        .node("build", started(1))
        .after(5)
        .node("build", usage(10, 0))
        .after(5)
        .node("build", failed(tokens(10, 0)))
        .after(10)
        .node("build", started(2))
        .after(5)
        .node("build", usage(7, 0))
        .build();

    assert_eq!(derive(&events).total_tokens(), tokens(10, 0));
    assert_eq!(live_total_tokens(&events), tokens(17, 0));
}

#[test]
fn an_attempt_restarted_with_no_terminal_leaves_its_usage_behind() {
    // Resume finds the node orphaned and starts it again, writing a
    // second `node_started` with no terminal between the two. The
    // interrupted attempt closed on nothing, so no `tokens_used` ever
    // claims its reports and no total carries them.
    let events = Log::for_run("run-1")
        .node("build", started(1))
        .after(5)
        .node("build", usage(100, 0))
        .after(15)
        .node("build", started(2))
        .after(5)
        .node("build", usage(7, 0))
        .build();

    assert_eq!(derive(&events).total_tokens(), TokenUsage::default());
    assert_eq!(live_total_tokens(&events), tokens(7, 0));
}

#[test]
fn usage_no_node_owns_is_left_out_of_the_live_total() {
    // No terminal event can ever retire it, so counting it would inflate
    // the run's total for good.
    let events = Log::for_run("run-1").event(usage(42, 7)).build();
    assert_eq!(live_total_tokens(&events), TokenUsage::default());
}

proptest! {
    /// A node's usage events and the `tokens_used` of its terminal
    /// describe the same tokens: whatever the adapter reported along the
    /// way, the live total is that one figure both before and after the
    /// node closes — never the two added together.
    #[test]
    fn usage_counts_once_across_a_nodes_terminal(
        reports in prop::collection::vec((0u64..1_000, 0u64..1_000), 1..8)
    ) {
        let spent = tokens(
            reports.iter().map(|(input, _)| input).sum(),
            reports.iter().map(|(_, output)| output).sum(),
        );

        let mut log = Log::for_run("run-1").node("build", started(1));
        for (index, (input, output)) in reports.iter().enumerate() {
            log = log
                .at(at(index as i64 + 1))
                .node("build", usage(*input, *output));
        }
        let events = log.at(at(100)).node("build", finished(spent)).build();

        // In flight: the terminal has not been written yet.
        let in_flight = &events[..events.len() - 1];
        prop_assert_eq!(live_total_tokens(in_flight), spent);

        prop_assert_eq!(derive(&events).total_tokens(), spent);
        prop_assert_eq!(live_total_tokens(&events), spent);
    }
}

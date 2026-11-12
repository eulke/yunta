use super::*;
use yunta_core::events::{
    AgentSessionOpenedPayload, ArtifactAcceptedPayload, ArtifactEvent, EventPayload, NodeEvent,
    NodeFailedPayload, NodeFinishedPayload, NodeOutcome, NodeStartedPayload, RecordedOrigin,
    RunEvent, RunFinishedPayload, RunMetrics, SessionEvent, StoredEvent, TerminalState, TokenUsage,
};
use yunta_core::{RunId, Workflow};
use yunta_replay::frame::RunPhase;
use yunta_testkit_core::{fixed_now, Log};

fn node_started(attempt: u32) -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(attempt)))
}

fn node_failed(text: &str) -> EventPayload {
    EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::Message {
            outcome: text.into(),
        },
        TokenUsage::default(),
    )))
}

fn node_finished() -> EventPayload {
    EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
        NodeOutcome::Summary("ok".into()),
        TokenUsage::default(),
    )))
}

fn run_failed() -> EventPayload {
    EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
        terminal_state: TerminalState::Failed,
        metrics: RunMetrics {
            cptv: None,
            tokens: TokenUsage::default(),
        },
        closed_by: None,
    }))
}

/// A finished run's report over `events`, as the run left its log.
fn finished_verdict(events: &[StoredEvent]) -> SessionOutcome {
    let state = yunta_replay::RunState::replay(events);
    verdict(&state, &Ending::Finished)
}

#[test]
fn accepted_questions_and_both_terminals_are_required_for_success() {
    let opened = EventPayload::Session(SessionEvent::Opened(AgentSessionOpenedPayload {
        session_id: "doctor-session".into(),
        agent: None,
        effort: None,
        model: None,
        capabilities: yunta_core::Capabilities::default(),
        fence: None,
        confinement: None,
        task_id: None,
        continues: None,
    }));
    let accepted = EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
        ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Questions,
        },
        yunta_core::sha256_hex(b"questions: []\n"),
        RecordedOrigin::Submitted,
    )));
    let finished = node_finished();
    let closed = EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
        terminal_state: TerminalState::Done,
        metrics: RunMetrics {
            cptv: None,
            tokens: TokenUsage::default(),
        },
        closed_by: None,
    }));
    let only_reply = Log::for_run("doctor-test")
        .node("probe", opened.clone())
        .build();
    assert!(matches!(
        finished_verdict(&only_reply),
        SessionOutcome::NoDelivery { .. }
    ));
    let delivered = Log::for_run("doctor-test")
        .node("probe", node_started(1))
        .node("probe", opened)
        .node("probe", accepted)
        .node("probe", finished)
        .event(closed)
        .build();
    assert!(matches!(
        finished_verdict(&delivered),
        SessionOutcome::Ok { .. }
    ));
}

#[test]
fn doctor_and_the_phase_page_name_the_same_last_failure() {
    // "build" failed and stayed failed. "probe" failed later, then ran again
    // to its finish. The page names the failure still standing; doctor must too.
    let events = Log::for_run("doctor-test")
        .node("build", node_started(1))
        .node("build", node_failed("build broke"))
        .node("probe", node_started(1))
        .node("probe", node_failed("probe broke"))
        .node("probe", node_started(2))
        .node("probe", node_finished())
        .event(run_failed())
        .build();
    let workflow: Workflow = yunta_core::yaml::parse(
        "name: doctor-test\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n  - { id: probe, kind: bash, run: \"true\" }\n",
    )
    .unwrap();
    let frame = yunta_replay::frame::RunFrame::of(
        &RunId::from("doctor-test"),
        &workflow,
        &yunta_replay::RunState::replay(&events),
        None,
        &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
        fixed_now(),
    );
    let RunPhase::Failed { failure: page } = frame.phase else {
        panic!("the run closed failed");
    };
    assert_eq!(ProbeFacts::from_events(&events).failure, page);
}

#[test]
fn the_probe_names_the_submit_tool_the_way_its_adapter_calls_it() {
    let claude = yunta_adapters::CLAUDE_CODE_ID.clone();
    let text = probe_workflow(&format!("Call {} with exactly", submit_called(&claude)));
    assert!(text.contains("prompt: 'Call mcp__yunta-run__"), "{text}");
}

#[test]
fn an_adapter_this_binary_does_not_build_names_the_submit_tool_bare() {
    let unbuilt = AdapterId::from_static("unbuilt");
    assert_eq!(
        submit_called(&unbuilt),
        RunTool::Submit(ArtifactKind::Questions).name()
    );
}

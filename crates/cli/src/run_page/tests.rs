use std::path::Path;

use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, EventPayload, NodeEvent, NodeFinishedPayload,
    NodeOutcome, NodeStartedPayload, SessionEvent, StoredEvent, TokenUsage,
};
use yunta_core::{Isolation, RunId, Workflow};
use yunta_engine::{derive, run_frame_of, RunFrame, RunState};
use yunta_testkit_core::{fixed_now, Log};

use super::{RunPage, Scope};
use crate::surface::Outline;

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

/// The workflow the fixture run follows: two nodes, neither of them parked.
fn workflow() -> Workflow {
    yunta_core::yaml::parse(
        "name: running\nnodes:\n  - { id: plan, kind: bash, run: \"true\" }\n  - { id: build, kind: bash, run: \"true\" }\n",
    )
    .expect("a workflow with two nodes")
}

/// The log of a run whose `plan` node settled 60 input tokens and whose
/// `build` node has 40 in flight, with no output.
fn spend_log() -> Vec<StoredEvent> {
    let usage = AgentMessagePayload {
        message_type: AgentMessageType::Usage,
        tool_name: None,
        target: None,
        input_tokens: Some(40),
        output_tokens: Some(0),
        cached_input_tokens: None,
        note: None,
        text: None,
    };
    let settled = NodeFinishedPayload::new(
        NodeOutcome::Summary("exit 0".to_string()),
        TokenUsage {
            input: 60,
            output: 0,
            cached: None,
        },
    );
    Log::for_run(RUN.as_str())
        .node(
            "plan",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node("plan", EventPayload::Node(NodeEvent::Finished(settled)))
        .node(
            "build",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node("build", EventPayload::Session(SessionEvent::Message(usage)))
        .build()
}

/// The page of the run `spend_log` writes. Both scopes are drawn from the
/// same frame.
fn running_page(scope: Scope) -> RunPage<'static> {
    let state: &'static RunState = Box::leak(Box::new(derive(&spend_log())));
    let frame: &'static RunFrame = Box::leak(Box::new(run_frame_of(
        &RUN,
        &workflow(),
        state,
        None,
        fixed_now(),
    )));
    let run_id: &'static RunId = Box::leak(Box::new(RUN));
    RunPage {
        scope,
        run_id,
        engine: crate::render::Engine::this(fixed_now()),
        frame,
        state,
        paths: outline(),
        decision: None,
    }
}

fn outline() -> Outline<'static> {
    Outline {
        run_dir: Path::new("/home/me/.yunta/runs/01JBZ5X8K3N7Q2W6E4R9T1Y0P5"),
        worktree: Path::new("/home/me/.yunta/worktrees/01JBZ5X8K3N7Q2W6E4R9T1Y0P5"),
        base_branch: "main",
        isolation: Isolation::Worktree,
        cwd: Path::new("/home/me/project"),
        home: Some(Path::new("/home/me")),
    }
}

#[test]
fn the_closing_and_the_status_page_show_one_tokens_figure() {
    assert_eq!(
        running_page(Scope::Status).tokens().as_deref(),
        Some("100 in / 0 out")
    );
    assert_eq!(
        running_page(Scope::Closing).tokens(),
        running_page(Scope::Status).tokens()
    );
}

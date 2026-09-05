//! `render_progress` — pure derivation from a
//! workflow and its event log, exercised directly without spinning up a
//! full run.

use yunta_core::events::{
    ArtifactWrittenPayload, EventBody, EventPayload, NodeFailedPayload, NodeFinishedPayload,
    NodeStartedPayload, StoredEvent, TokenUsage,
};
use yunta_core::{Node, NodeKind, PromptSource, Workflow};
use yunta_engine::render_progress;

fn node(id: &str, description: Option<&str>) -> Node {
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
        on_failure: None,
        on_interrupt: None,
        description: description.map(str::to_string),
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

fn prompt_node(id: &str) -> Node {
    Node {
        kind: NodeKind::Prompt {
            prompt: PromptSource::Inline("do it".to_string()),
        },
        ..node(id, None)
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

fn event(seq: u64, node_id: &str, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        run_id: "run-1".into(),
        seq: seq.into(),
        timestamp: chrono::Utc::now(),
        node_id: Some(node_id.into()),
        body: EventBody::Known(payload),
    }
}

fn tokens() -> TokenUsage {
    TokenUsage {
        input: 1,
        output: 1,
        cached: None,
    }
}

#[test]
fn a_workflow_with_no_events_yet_lists_every_node_as_next() {
    let wf = workflow(vec![node("plan", None), node("implement", Some("Do it"))]);
    let markdown = render_progress(&wf, &[]);

    assert_eq!(
        markdown,
        "# Progress\n\n## Finished\n\n_none yet_\n\n## Failed\n\n_none_\n\n## Next\n\n- **plan** — plan\n- **implement** — Do it\n"
    );
}

#[test]
fn a_finished_node_shows_its_description_outcome_and_artifacts() {
    let wf = workflow(vec![node("plan", Some("Writes the ledger"))]);
    let events = vec![
        event(
            1,
            "plan",
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            "plan",
            EventPayload::ArtifactWritten(ArtifactWrittenPayload {
                path: "artifacts/ledger.yaml".into(),
                content_hash: yunta_core::sha256_hex(b"deadbeef"),
                artifact_kind: None,
            }),
        ),
        event(
            3,
            "plan",
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "planned".to_string(),
                tokens_used: tokens(),
            }),
        ),
    ];

    let markdown = render_progress(&wf, &events);

    assert_eq!(
        markdown,
        "# Progress\n\n## Finished\n\n- **plan** — Writes the ledger\n  outcome: planned\n  artifact: artifacts/ledger.yaml\n\n## Failed\n\n_none_\n\n## Next\n\n_nothing pending_\n"
    );
}

#[test]
fn a_failed_node_appears_under_failed_with_its_outcome() {
    let wf = workflow(vec![node("lint", None)]);
    let events = vec![
        event(
            1,
            "lint",
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            "lint",
            EventPayload::NodeFailed(NodeFailedPayload {
                outcome: "clippy: 3 warnings".to_string(),
                tokens_used: TokenUsage::default(),
                retryable: false,
            }),
        ),
    ];

    let markdown = render_progress(&wf, &events);
    assert_eq!(
        markdown,
        "# Progress\n\n## Finished\n\n_none yet_\n\n## Failed\n\n- **lint** — clippy: 3 warnings\n\n## Next\n\n_nothing pending_\n"
    );
}

#[test]
fn a_running_node_appears_under_next_marked_running() {
    let wf = workflow(vec![prompt_node("implement")]);
    let events = vec![event(
        1,
        "implement",
        EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
    )];

    let markdown = render_progress(&wf, &events);
    assert_eq!(
        markdown,
        "# Progress\n\n## Finished\n\n_none yet_\n\n## Failed\n\n_none_\n\n## Next\n\n- **implement** — implement (running)\n"
    );
}

#[test]
fn parallel_children_are_listed_on_the_same_terms_as_top_level_nodes() {
    let wf = workflow(vec![Node {
        kind: NodeKind::Parallel {
            coordination: yunta_core::Coordination::Independent,
            join: yunta_core::JoinPolicy::All,
            nodes: vec![node("write-docs", Some("Writes docs"))],
        },
        ..node("pre-launch", None)
    }]);

    let events = vec![
        event(
            1,
            "write-docs",
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            "write-docs",
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "exit 0".to_string(),
                tokens_used: TokenUsage::default(),
            }),
        ),
    ];

    let markdown = render_progress(&wf, &events);
    assert_eq!(
        markdown,
        "# Progress\n\n## Finished\n\n- **write-docs** — Writes docs\n  outcome: exit 0\n\n## Failed\n\n_none_\n\n## Next\n\n- **pre-launch** — pre-launch\n"
    );
}

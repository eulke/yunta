//! `render_progress` (T5.5, Contrato §8.2) — pure derivation from a
//! workflow and its event log, exercised directly without spinning up a
//! full run.

use yunta_core::events::{
    ArtifactWrittenPayload, Event, EventPayload, NodeFailedPayload, NodeFinishedPayload,
    NodeStartedPayload, TokenUsage,
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
        name: "fixture".to_string(),
        description: None,
        node_defaults: None,
        nodes,
    }
}

fn event(seq: u64, node_id: &str, payload: EventPayload) -> Event {
    Event {
        run_id: "run-1".into(),
        seq,
        timestamp: chrono::Utc::now(),
        node_id: Some(node_id.into()),
        payload,
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

    assert!(markdown.contains("## Next"));
    assert!(markdown.contains("- **plan** — plan\n"));
    assert!(markdown.contains("- **implement** — Do it\n"));
    assert!(markdown.contains("_none yet_"), "Finished section is empty");
    assert!(markdown.contains("_none_"), "Failed section is empty");
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
                content_hash: "deadbeef".to_string(),
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

    let finished = markdown.split("## Finished").nth(1).unwrap();
    assert!(finished.contains("- **plan** — Writes the ledger"));
    assert!(finished.contains("outcome: planned"));
    assert!(finished.contains("artifact: artifacts/ledger.yaml"));
    let next = markdown.split("## Next").nth(1).unwrap();
    assert!(
        !next.contains("**plan**"),
        "plan should not also appear under Next"
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
    let failed = markdown.split("## Failed").nth(1).unwrap();
    assert!(failed.contains("- **lint** — clippy: 3 warnings"));
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
    let next = markdown.split("## Next").nth(1).unwrap();
    assert!(next.contains("- **implement** — implement (running)"));
}

#[test]
fn parallel_children_are_listed_on_the_same_terms_as_top_level_nodes() {
    let wf = workflow(vec![Node {
        kind: NodeKind::Parallel {
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
    let finished = markdown.split("## Finished").nth(1).unwrap();
    assert!(finished.contains("- **write-docs** — Writes docs"));
    let next = markdown.split("## Next").nth(1).unwrap();
    assert!(next.contains("- **pre-launch** — pre-launch\n"));
}

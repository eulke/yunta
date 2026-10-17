//! A node whose session fails after it asked a question: the node fails for
//! the reason it failed, and waits for no one.

use super::*;

#[tokio::test]
async fn a_node_failed_after_a_questions_artifact_derives_failed_not_waiting() {
    // The log of the production failure: a node hands over its questions and
    // then fails for an unrelated reason. The failure is a failure — a node
    // waits because it asked, never because a `questions` artifact exists.
    let bench = Bench::new();
    let outcome = "scope violated: 1 file outside";
    let events = Log::for_run(bench.run_id.as_str())
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Started(
                yunta_core::events::NodeStartedPayload::attempt(1),
            )),
        )
        .node(
            "grill",
            EventPayload::Artifacts(ArtifactEvent::Accepted(
                yunta_core::events::ArtifactAcceptedPayload::new(
                    yunta_core::events::ArtifactId::Interpreted {
                        kind: yunta_core::ArtifactKind::Questions,
                    },
                    yunta_core::sha256_hex(b"questions"),
                    yunta_core::events::RecordedOrigin::Submitted,
                ),
            )),
        )
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Failed(
                yunta_core::events::NodeFailedPayload::new(
                    yunta_core::events::Failure::Message {
                        outcome: outcome.into(),
                    },
                    Default::default(),
                ),
            )),
        )
        .build();

    let state = derive(&events);
    assert!(
        matches!(state.nodes.state("grill"), Some(NodeState::Failed { .. })),
        "a node that failed is failed, whatever artifacts it holds: got {:?}",
        state.nodes.state("grill")
    );
}

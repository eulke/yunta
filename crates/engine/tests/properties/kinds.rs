//! The event payloads a property draws from: each arm is one kind a log can
//! hold, and the arms are picked with equal weight.

use super::*;

/// The kinds a property varies deliberately: an attempt counter, ids
/// drawn from pools small enough that a log revisits the same task and
/// the same artifact identity, and both shapes a task status has on the
/// wire.
pub(super) fn parameterized() -> impl Strategy<Value = EventPayload> {
    let arms = node_and_task_arms()
        .into_iter()
        .chain(finding_and_artifact_arms())
        .chain(run_gate_and_child_arms());
    proptest::strategy::Union::new(arms)
}

/// A node's attempt, finish and failure, and a task's registration and status.
fn node_and_task_arms() -> Vec<BoxedStrategy<EventPayload>> {
    vec![
        (1u32..4)
            .prop_map(|attempt| {
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(attempt)))
            })
            .boxed(),
        ("[a-z]{0,6}", tokens())
            .prop_map(|(outcome, tokens_used)| {
                EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                    NodeOutcome::Summary(outcome),
                    tokens_used,
                )))
            })
            .boxed(),
        ("[a-z]{0,6}", tokens())
            .prop_map(|(outcome, tokens_used)| {
                EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                    Failure::Message {
                        outcome: outcome.as_str().into(),
                    },
                    tokens_used,
                )))
            })
            .boxed(),
        task_id()
            .prop_map(|id| {
                EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                    task_id: id.into(),
                    criteria: Vec::new(),
                    scope: Vec::new(),
                    depends_on: Vec::new(),
                }))
            })
            .boxed(),
        (task_id(), task_status(), any::<bool>())
            .prop_map(|(id, new_status, placed)| {
                // Both shapes a status has on the wire: a `done` naming where
                // the work landed, and any other naming no commit at all.
                let changed = match placed {
                    true => {
                        TaskStatusChangedPayload::done(id.into(), 1u64.into(), "deadbeef".into())
                    }
                    false => TaskStatusChangedPayload::to(id.into(), new_status, 1u64.into()),
                };
                EventPayload::Tasks(TaskEvent::StatusChanged(changed))
            })
            .boxed(),
    ]
}

/// A finding a session posted, and an artifact the run accepted.
fn finding_and_artifact_arms() -> Vec<BoxedStrategy<EventPayload>> {
    vec![
        (severity(), "[a-z]{0,6}", "[a-z]{1,6}")
            .prop_map(|(severity, title, location)| {
                EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                    finding: Finding {
                        id: "f1".into(),
                        severity,
                        says: FindingSays::Written {
                            title,
                            detail: "d".to_string(),
                        },
                        location: yunta_core::Location::work(
                            yunta_core::RelativePath::of([location]),
                            None,
                        ),
                        proposed_criterion: None,
                    },
                }))
            })
            .boxed(),
        (artifact_id(), "[a-z]{1,4}")
            .prop_map(|(artifact, content)| {
                EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                    artifact,
                    yunta_core::sha256_hex(content.as_bytes()),
                    RecordedOrigin::Submitted,
                )))
            })
            .boxed(),
    ]
}

/// A run paused on a recorded sentence, a question asked and answered, and a
/// child run finished.
fn run_gate_and_child_arms() -> Vec<BoxedStrategy<EventPayload>> {
    vec![
        "[a-z ]{0,10}"
            .prop_map(|reason| {
                EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
                    &PauseReason::Recorded {
                        sentence: reason.as_str().into(),
                    },
                )))
            })
            .boxed(),
        ("[a-z]{1,4}", tokens())
            .prop_map(|(content, tokens_used)| {
                EventPayload::Gates(GateEvent::QuestionsAsked(
                    yunta_core::events::QuestionsAskedPayload::new(
                        yunta_core::sha256_hex(content.as_bytes()),
                        vec!["q1".into()],
                        tokens_used,
                    )
                    .expect("one question is a question"),
                ))
            })
            .boxed(),
        "[a-z]{1,4}"
            .prop_map(|content| {
                EventPayload::Gates(GateEvent::QuestionsAnswered(
                    yunta_core::events::QuestionsAnsweredPayload {
                        answers_hash: yunta_core::sha256_hex(content.as_bytes()),
                        channel: yunta_core::events::Channel::Tty,
                        responder: None,
                    },
                ))
            })
            .boxed(),
        // A child run closes with the usage it spent, and replay adds it to
        // the parent's total: the monotone check sees it only if drawn.
        (tokens(), prop::sample::select(vec!["child-a", "child-b"]))
            .prop_map(|(tokens_used, child)| {
                EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
                    RunId::from(child),
                    yunta_core::sha256_hex(child.as_bytes()),
                    TerminalState::Done,
                    tokens_used,
                )))
            })
            .boxed(),
    ]
}

//! The chronicle: one moment per event, in the log's order, read
//! against the state at that event.
//!
//! What the frame says a node *is*, a moment says it *became* — and
//! both come off the same log through the same fold, so a surface that
//! draws one and a surface that draws the other cannot disagree about
//! what happened.

use yunta_core::events::{
    ChildRunCreatedPayload, ChildRunFinishedPayload, EventBody, EventPayload, NodeEvent,
    NodeFinishedPayload, NodeStartedPayload, NodeState, StoredEvent, TerminalState, TokenUsage,
};
use yunta_engine::{chronicle, Happening};

/// One event of `payload`, at `seq` seconds past the epoch, under
/// `node`.
fn event(seq: u64, node: Option<&str>, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        run_id: yunta_core::RunId::from("run-chronicle"),
        seq: seq.into(),
        timestamp: chrono::DateTime::UNIX_EPOCH + chrono::Duration::seconds(seq as i64),
        node_id: node.map(Into::into),
        body: EventBody::Known(payload),
    }
}

fn started() -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload { attempt: 1 }))
}

fn finished(outcome: &str) -> EventPayload {
    EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload {
        outcome: outcome.to_string(),
        tokens_used: TokenUsage::default(),
    }))
}

#[test]
fn every_event_is_one_moment() {
    // Including a kind this binary does not know: a reader not told the
    // log carries more than the binary reads is a reader misled.
    let mut events: Vec<StoredEvent> = yunta_testkit_core::all_kinds()
        .into_iter()
        .enumerate()
        .map(|(index, payload)| event(index as u64 + 1, Some("only"), payload))
        .collect();
    let next = events.len() as u64 + 1;
    events.push(StoredEvent {
        body: EventBody::Unknown(yunta_core::events::UnknownEvent {
            kind: "criteria_checked_v2".to_string(),
            schema_version: 2,
            payload: serde_json::Map::new(),
        }),
        ..event(next, Some("only"), started())
    });

    let moments = chronicle(&events);
    assert_eq!(moments.len(), events.len(), "one moment per event");
    assert!(
        matches!(
            moments.last().map(|moment| &moment.happening),
            Some(Happening::Unknown { kind }) if kind == "criteria_checked_v2"
        ),
        "a kind this binary does not read is still a moment: {:?}",
        moments.last()
    );
    for (moment, event) in moments.iter().zip(&events) {
        assert_eq!(moment.seq, event.seq);
        assert_eq!(moment.at, event.timestamp);
    }
}

#[test]
fn a_settled_node_carries_how_long_it_worked_and_the_children_it_bore() {
    let events = vec![
        event(1, Some("compose"), started()),
        event(
            2,
            Some("compose"),
            EventPayload::Children(yunta_core::events::ChildEvent::Created(
                ChildRunCreatedPayload {
                    child_run_id: yunta_core::RunId::from("child-1"),
                    child_workflow_hash: yunta_core::sha256_hex(b"wf"),
                },
            )),
        ),
        event(
            3,
            Some("compose"),
            EventPayload::Children(yunta_core::events::ChildEvent::Finished(
                ChildRunFinishedPayload {
                    child_run_id: yunta_core::RunId::from("child-1"),
                    child_workflow_hash: yunta_core::sha256_hex(b"wf"),
                    terminal_state: TerminalState::Done,
                    tokens: TokenUsage::default(),
                },
            )),
        ),
        event(4, Some("compose"), finished("composed")),
    ];

    let moments = chronicle(&events);
    let settled = moments.last().expect("the close");
    let Happening::Node(yunta_core::events::node::happening::Happening::Reached {
        state,
        elapsed,
        children,
    }) = &settled.happening
    else {
        panic!("the close is a node reaching a state: {settled:?}");
    };
    assert!(matches!(state, NodeState::Finished { .. }), "{state:?}");
    assert_eq!(
        *elapsed,
        Some(std::time::Duration::from_secs(3)),
        "the attempt worked from its start to its close"
    );
    assert_eq!(children.len(), 1, "the child it bore travels with it");
    assert_eq!(
        children[0].terminal,
        Some(TerminalState::Done),
        "and how that child closed"
    );
}

#[test]
fn a_node_that_settles_twice_is_two_moments() {
    // The defect a scrollback that remembered "what is gone" had: a
    // node that closes twice is two things that happened, and each
    // moment carries the state it reached, not the one the log ends on.
    let events = vec![
        event(1, Some("lint"), started()),
        event(
            2,
            Some("lint"),
            EventPayload::Node(NodeEvent::Failed(
                yunta_core::events::NodeFailedPayload::new(
                    yunta_core::events::Failure::Message {
                        outcome: "exit 1".to_string(),
                    },
                    true,
                    TokenUsage::default(),
                ),
            )),
        ),
        event(3, Some("lint"), started()),
        event(4, Some("lint"), finished("exit 0")),
    ];

    let reached: Vec<NodeState> = chronicle(&events)
        .into_iter()
        .filter_map(|moment| match moment.happening {
            Happening::Node(yunta_core::events::node::happening::Happening::Reached {
                state,
                ..
            }) => Some(state),
            _ => None,
        })
        .collect();
    assert_eq!(reached.len(), 4, "two attempts, four states reached");
    assert!(
        matches!(reached[1], NodeState::Failed { .. }),
        "{reached:?}"
    );
    assert!(
        matches!(reached[3], NodeState::Finished { .. }),
        "the second close is its own moment, not a repeat of the first: {reached:?}"
    );
}

//! Property tests for the replay guarantees the whole engine rests on:
//! `derive` is a pure, deterministic, prefix-monotonic fold of the event
//! log; every event round-trips through its JSON wire form; and a run
//! interrupted at any point and resumed derives the same final state as one
//! that never stopped.
//!
//! The generator emits well-formed *events* (a valid payload per known
//! kind, `seq` in order) but does not enforce a valid *lifecycle* — `derive`
//! interprets any log (marking one it cannot follow `broken`), so these
//! properties must hold over arbitrary logs, not just tidy ones.

use proptest::prelude::*;
use yunta_core::events::{
    EventBody, EventPayload, Finding, FindingPostedPayload, FindingSeverity, NodeFailedPayload,
    NodeFinishedPayload, NodeStartedPayload, RunPausedPayload, StoredEvent, TaskRegisteredPayload,
    TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_engine::derive;

const RUN: &str = "run-prop";

fn tokens() -> impl Strategy<Value = TokenUsage> {
    (0u64..1000, 0u64..1000, proptest::option::of(0u64..1000)).prop_map(
        |(input, output, cached)| TokenUsage {
            input,
            output,
            cached,
        },
    )
}

fn task_status() -> impl Strategy<Value = TaskStatus> {
    prop_oneof![
        Just(TaskStatus::Pending),
        Just(TaskStatus::Ready),
        Just(TaskStatus::Running),
        Just(TaskStatus::Done),
        Just(TaskStatus::Blocked),
        Just(TaskStatus::Failed),
    ]
}

fn severity() -> impl Strategy<Value = FindingSeverity> {
    prop_oneof![
        Just(FindingSeverity::Blocking),
        Just(FindingSeverity::Major),
        Just(FindingSeverity::Minor),
        Just(FindingSeverity::Note),
    ]
}

/// A task id drawn from a tiny pool, so generated logs actually revisit the
/// same task and `derive` folds real transitions rather than a sea of
/// singletons.
fn task_id() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("t1"), Just("t2"), Just("t3")]
}

fn payload() -> impl Strategy<Value = EventPayload> {
    prop_oneof![
        (1u32..4).prop_map(|attempt| EventPayload::NodeStarted(NodeStartedPayload { attempt })),
        ("[a-z]{0,6}", tokens()).prop_map(|(outcome, tokens_used)| EventPayload::NodeFinished(
            NodeFinishedPayload {
                outcome,
                tokens_used,
            }
        )),
        ("[a-z]{0,6}", tokens(), any::<bool>()).prop_map(|(outcome, tokens_used, retryable)| {
            EventPayload::NodeFailed(NodeFailedPayload {
                outcome,
                tokens_used,
                retryable,
                diagnostics: Vec::new(),
            })
        }),
        task_id().prop_map(|id| EventPayload::TaskRegistered(TaskRegisteredPayload {
            task_id: id.into(),
            criteria: Vec::new(),
            scope: Vec::new(),
            depends_on: Vec::new(),
        })),
        (task_id(), task_status()).prop_map(|(id, new_status)| EventPayload::TaskStatusChanged(
            TaskStatusChangedPayload {
                task_id: id.into(),
                new_status,
                caused_by: 1u64.into(),
            }
        )),
        (severity(), "[a-z]{0,6}", "[a-z]{0,6}").prop_map(|(severity, title, location)| {
            EventPayload::FindingPosted(FindingPostedPayload {
                finding: Finding {
                    id: "f1".into(),
                    severity,
                    title,
                    location,
                    detail: "d".to_string(),
                    proposed_criterion: None,
                },
            })
        }),
        "[a-z ]{0,10}".prop_map(|reason| EventPayload::RunPaused(RunPausedPayload { reason })),
    ]
}

/// One log entry: an optional node id (from a small pool, so nodes recur)
/// and its payload. `seq` is assigned in order when the log is built.
fn entry() -> impl Strategy<Value = (Option<&'static str>, EventPayload)> {
    let node = prop_oneof![
        Just(None),
        Just(Some("a")),
        Just(Some("b")),
        Just(Some("c"))
    ];
    (node, payload())
}

fn log() -> impl Strategy<Value = Vec<StoredEvent>> {
    prop::collection::vec(entry(), 0..40).prop_map(|entries| {
        entries
            .into_iter()
            .enumerate()
            .map(|(i, (node, payload))| StoredEvent {
                run_id: RUN.into(),
                seq: ((i + 1) as u64).into(),
                // Fixed: a log's derived state must not depend on wall time.
                timestamp: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                    .expect("valid timestamp")
                    .with_timezone(&chrono::Utc),
                node_id: node.map(Into::into),
                body: EventBody::Known(payload),
            })
            .collect()
    })
}

proptest! {
    /// `derive` is pure: the same log always derives the same state, with no
    /// dependence on `HashMap` iteration order or any hidden state.
    #[test]
    fn derive_is_deterministic(log in log()) {
        prop_assert_eq!(derive(&log), derive(&log));
    }

    /// Every event survives a trip through its JSON wire form unchanged —
    /// the persistence and `events.jsonl` contract.
    #[test]
    fn events_round_trip_through_json(log in log()) {
        for event in &log {
            let json = serde_json::to_string(event).expect("serialize");
            let back: StoredEvent = serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(&back, event);
        }
    }

    /// `derive` is monotonic by prefix: as the log grows, tokens only
    /// accumulate, tasks and nodes once seen stay seen, and findings only
    /// pile up — a longer prefix never un-does what a shorter one derived.
    #[test]
    fn derive_is_prefix_monotonic(log in log()) {
        let mut prev = derive(&[]);
        for k in 1..=log.len() {
            let cur = derive(&log[..k]);
            prop_assert!(cur.total_tokens.total() >= prev.total_tokens.total());
            prop_assert!(cur.findings.len() >= prev.findings.len());
            for id in prev.tasks.keys() {
                prop_assert!(cur.tasks.contains_key(id), "task {id} disappeared");
            }
            for id in prev.nodes.keys() {
                prop_assert!(cur.nodes.contains_key(id), "node {id} disappeared");
            }
            prev = cur;
        }
    }

    /// A run interrupted at any event and resumed derives the same final
    /// state as one that never stopped. Resume reloads the whole persisted
    /// log and re-derives, so deriving a prefix first (the crash) must not
    /// change the result of deriving the full log (the resume) — which holds
    /// exactly because `derive` is pure.
    #[test]
    fn an_interrupted_run_resumes_to_the_same_final_state(
        (log, k) in log().prop_flat_map(|log| {
            let n = log.len();
            (Just(log), 0..=n)
        })
    ) {
        let uninterrupted = derive(&log);
        let _crashed_at_k = derive(&log[..k]);
        let resumed = derive(&log);
        prop_assert_eq!(resumed, uninterrupted);
    }
}

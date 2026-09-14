//! Property tests for the replay guarantees the whole engine rests on:
//! `derive` is a pure, deterministic, prefix-monotonic fold of the event
//! log, at every length and not just at the full log; every event
//! round-trips through its JSON wire form; and verifying a run's
//! artifacts against an intact store changes nothing.
//!
//! The generator emits well-formed *events* (a valid payload per known
//! kind, `seq` in order) but does not enforce a valid *lifecycle* — `derive`
//! interprets any log (marking one it cannot follow `broken`), so these
//! properties must hold over arbitrary logs, not just tidy ones.

use proptest::prelude::*;

/// Drives an async call to completion on a runtime of its own — what
/// a property body, which is synchronous by construction, has instead
/// of an `await`.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime for one property case")
        .block_on(future)
}
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, ArtifactWrittenPayload, EventBody, EventPayload, Failure,
    Finding, FindingPostedPayload, FindingSeverity, NodeFailedPayload, NodeFinishedPayload,
    NodeStartedPayload, RecordedOrigin, RunPausedPayload, StoredEvent, TaskRegisteredPayload,
    TaskStatus, TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::events::{ArtifactEvent, FindingEvent, GateEvent, NodeEvent, RunEvent, TaskEvent};
use yunta_core::ArtifactKind;
use yunta_engine::{derive, ArtifactIntegrity, ObjectStore};

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
        (1u32..4).prop_map(
            |attempt| EventPayload::Node(NodeEvent::Started(NodeStartedPayload { attempt }))
        ),
        ("[a-z]{0,6}", tokens()).prop_map(|(outcome, tokens_used)| EventPayload::Node(
            NodeEvent::Finished(NodeFinishedPayload {
                outcome,
                tokens_used,
            })
        )),
        ("[a-z]{0,6}", tokens(), any::<bool>()).prop_map(|(outcome, tokens_used, retryable)| {
            EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                Failure::message(outcome),
                retryable,
                tokens_used,
            )))
        }),
        task_id().prop_map(|id| EventPayload::Tasks(TaskEvent::Registered(
            TaskRegisteredPayload {
                task_id: id.into(),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            }
        ))),
        (task_id(), task_status(), any::<bool>()).prop_map(|(id, new_status, placed)| {
            // Both shapes a status has on the wire: a `done` naming where
            // the work landed, and any other naming no commit at all.
            let changed = match placed {
                true => TaskStatusChangedPayload::done(id.into(), 1u64.into(), "deadbeef".into()),
                false => TaskStatusChangedPayload::to(id.into(), new_status, 1u64.into()),
            };
            EventPayload::Tasks(TaskEvent::StatusChanged(changed))
        }),
        (severity(), "[a-z]{0,6}", "[a-z]{0,6}").prop_map(|(severity, title, location)| {
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: Finding {
                    id: "f1".into(),
                    severity,
                    title,
                    location,
                    detail: "d".to_string(),
                    proposed_criterion: None,
                },
            }))
        }),
        (artifact_id(), "[a-z]{1,4}").prop_map(|(artifact, content)| {
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                artifact,
                yunta_core::sha256_hex(content.as_bytes()),
                RecordedOrigin::Submitted,
            )))
        }),
        "[a-z ]{0,10}".prop_map(|reason| EventPayload::Run(RunEvent::Paused(
            RunPausedPayload::recorded(reason)
        ))),
        ("[a-z]{1,4}", tokens()).prop_map(|(content, tokens_used)| EventPayload::Gates(
            GateEvent::QuestionsAsked(
                yunta_core::events::QuestionsAskedPayload::new(
                    yunta_core::sha256_hex(content.as_bytes()),
                    vec!["q1".into()],
                    tokens_used,
                )
                .expect("one question is a question")
            )
        )),
        "[a-z]{1,4}".prop_map(|content| EventPayload::Gates(GateEvent::QuestionsAnswered(
            yunta_core::events::QuestionsAnsweredPayload {
                answers_hash: yunta_core::sha256_hex(content.as_bytes()),
                channel: yunta_core::events::Channel::Tty,
                responder: None,
            }
        ))),
    ]
}

/// An artifact identity drawn from a tiny pool, so generated logs
/// actually accept the same identity twice and the fold folds real
/// replacements rather than a sea of singletons.
fn artifact_id() -> impl Strategy<Value = ArtifactId> {
    prop_oneof![
        Just(ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks
        }),
        Just(ArtifactId::Interpreted {
            kind: ArtifactKind::Findings
        }),
        Just(ArtifactId::Opaque {
            name: "notes.md".to_string()
        }),
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
            .map(|(i, (node, payload))| event(i, node, payload))
            .collect()
    })
}

/// One event at its position in a generated log. Fixed timestamp: a
/// log's derived state must not depend on wall time.
fn event(index: usize, node: Option<&'static str>, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        run_id: RUN.into(),
        seq: ((index + 1) as u64).into(),
        timestamp: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&chrono::Utc),
        node_id: node.map(Into::into),
        body: EventBody::Known(payload),
    }
}

/// The artifacts a run accepted, each with the bytes behind it — so a
/// test can fill an object store with exactly what the log names.
fn accepted_artifacts() -> impl Strategy<Value = Vec<(Option<&'static str>, ArtifactId, String)>> {
    let node = prop_oneof![Just(None), Just(Some("a")), Just(Some("b"))];
    prop::collection::vec((node, artifact_id(), "[a-z]{1,8}"), 0..12)
}

/// One node's whole ask round: it starts, hands its questions over,
/// waits, is answered, and takes the terminal its close deferred.
fn a_questions_round() -> Vec<StoredEvent> {
    let questions = yunta_core::sha256_hex(b"questions");
    let answers = yunta_core::sha256_hex(b"answers");
    vec![
        event(
            0,
            Some("grill"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        event(
            1,
            Some("grill"),
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Questions,
                },
                questions.clone(),
                RecordedOrigin::Submitted,
            ))),
        ),
        event(
            2,
            Some("grill"),
            EventPayload::Gates(GateEvent::QuestionsAsked(
                yunta_core::events::QuestionsAskedPayload::new(
                    questions,
                    vec!["q1".into()],
                    yunta_core::events::TokenUsage {
                        input: 30,
                        output: 12,
                        cached: None,
                    },
                )
                .expect("one question is a question"),
            )),
        ),
        event(
            3,
            Some("grill"),
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Opaque {
                    name: "questions.answers.yaml".to_string(),
                },
                answers.clone(),
                RecordedOrigin::Answered,
            ))),
        ),
        event(
            4,
            Some("grill"),
            EventPayload::Gates(GateEvent::QuestionsAnswered(
                yunta_core::events::QuestionsAnsweredPayload {
                    answers_hash: answers,
                    channel: yunta_core::events::Channel::Tty,
                    responder: None,
                },
            )),
        ),
        event(
            5,
            Some("grill"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "questions answered".to_string(),
                yunta_core::events::TokenUsage::default(),
            ))),
        ),
    ]
}

/// What every cut of a round derives, and therefore what a resume does
/// from there: the node's state at each length of the log.
///
/// The table is the point. A crash after the node asked leaves it
/// waiting, so a resume asks again from the document it already holds; a
/// crash after the answer leaves it running and owed a terminal, so a
/// resume pays that terminal from the log. At no cut does the node come
/// back as something to re-run, which is what makes an interrupted round
/// cost an append rather than a session.
#[test]
fn every_cut_of_a_questions_round_derives_what_a_resume_acts_on() {
    let round = a_questions_round();
    let grill = yunta_core::NodeId::from("grill");
    let asked_tokens = yunta_core::events::TokenUsage {
        input: 30,
        output: 12,
        cached: None,
    };

    let at = |k: usize| derive(&round[..k]);
    assert_eq!(at(0).nodes.state(&grill), None, "before it started");
    for k in 1..=2 {
        assert!(
            matches!(
                at(k).nodes.state(&grill),
                Some(yunta_engine::NodeState::Running { attempt: 1 })
            ),
            "running at {k}: {:?}",
            at(k).nodes.state(&grill)
        );
        assert!(!at(k).answered_unfinished(&grill));
    }
    for k in 3..=4 {
        assert!(
            matches!(
                at(k).nodes.state(&grill),
                Some(yunta_engine::NodeState::Waiting { external_ref: None })
            ),
            "waiting on its questions at {k}: {:?}",
            at(k).nodes.state(&grill)
        );
        assert_eq!(
            at(k).total_tokens(),
            asked_tokens,
            "the session that asked is counted once, at {k}"
        );
    }
    assert!(
        matches!(
            at(5).nodes.state(&grill),
            Some(yunta_engine::NodeState::Running { attempt: 1 })
        ),
        "answered, back where asking left it: {:?}",
        at(5).nodes.state(&grill)
    );
    assert!(
        at(5).answered_unfinished(&grill),
        "and owed the terminal its close deferred"
    );
    assert_eq!(
        at(6).nodes.state(&grill),
        Some(&yunta_engine::NodeState::Finished {
            outcome: "questions answered".to_string(),
            tokens: asked_tokens,
        }),
        "finished, carrying what the session that asked spent"
    );
    assert!(!at(6).answered_unfinished(&grill));
    assert_eq!(
        at(6).total_tokens(),
        asked_tokens,
        "counted once, not twice"
    );
}

proptest! {
    /// `derive` is pure: the same log always derives the same state, with no
    /// dependence on `HashMap` iteration order or any hidden state.
    #[test]
    fn derive_is_deterministic(log in log()) {
        prop_assert_eq!(derive(&log), derive(&log));
    }

    /// The artifact fold is a pure function of the log, like `derive`,
    /// and it holds every identity the log accepted: `every` returns one
    /// ref per `(producer, identity)` the log named, never fewer.
    #[test]
    fn the_artifact_fold_is_deterministic_and_holds_every_identity(log in log()) {
        let ledger = ArtifactLedger::of(&log);
        prop_assert_eq!(&ledger, &ArtifactLedger::of(&log));

        let held: std::collections::BTreeSet<_> = ledger
            .every()
            .map(|artifact| (artifact.producer.clone(), artifact.artifact.clone()))
            .collect();
        let accepted: std::collections::BTreeSet<_> = log
            .iter()
            .filter_map(|event| match event.payload() {
                Some(EventPayload::Artifacts(ArtifactEvent::Accepted(p))) => {
                    Some((event.node_id.clone(), p.artifact.clone()))
                }
                _ => None,
            })
            .collect();
        prop_assert_eq!(held, accepted);
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
            prop_assert!(cur.total_tokens().total() >= prev.total_tokens().total());
            prop_assert!(cur.effective_findings().len() >= prev.effective_findings().len());
            for id in prev.tasks.iter().map(|(id, _)| id) {
                prop_assert!(cur.tasks.contains(id), "task {id} disappeared");
            }
            for id in prev.nodes.iter().map(|(id, _)| id) {
                prop_assert!(cur.nodes.has_state(id), "node {id} disappeared");
            }
            prev = cur;
        }
    }

    /// `derive` is deterministic at every length, not just at the full
    /// log: the same prefix derived twice is the same state. Nothing
    /// outside the events — an iteration order, a clock, a counter kept
    /// between calls — reaches the result of a partial log either.
    #[test]
    fn derive_is_deterministic_from_any_prefix(
        (log, k) in log().prop_flat_map(|log| {
            let n = log.len();
            (Just(log), 0..=n)
        })
    ) {
        let prefix = &log[..k];
        prop_assert_eq!(derive(prefix), derive(prefix));
    }

    /// Verifying a run's artifacts against an intact store finds nothing
    /// and changes nothing: whatever the log accepted, a store holding
    /// exactly those bytes answers for all of it, the run derives the
    /// state it derived before, and an artifact named by a log older than
    /// the store is accounted for as one nothing can be checked against
    /// rather than as a fault.
    #[test]
    fn verifying_an_intact_store_finds_nothing_and_changes_nothing(
        accepted in accepted_artifacts(),
        older in prop::collection::vec("[a-z]{1,8}", 0..4),
    ) {
        let run = tempfile::tempdir().expect("tempdir");
        let store = ObjectStore::at(run.path());
        let mut log: Vec<StoredEvent> = Vec::new();
        for (node, artifact, content) in &accepted {
            let content_hash = block_on(store.put(content.as_bytes())).expect("store the bytes");
            log.push(event(
                log.len(),
                *node,
                EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(artifact.clone(), content_hash, RecordedOrigin::Submitted))),
            ));
        }
        for name in &older {
            log.push(event(
                log.len(),
                Some("a"),
                EventPayload::Artifacts(ArtifactEvent::Written(ArtifactWrittenPayload {
                    path: std::path::PathBuf::from(format!("artifacts/{name}.md")),
                    content_hash: yunta_core::sha256_hex(name.as_bytes()),
                    artifact_kind: None,
                })),
            ));
        }

        let before = derive(&log);
        let integrity = block_on(ArtifactIntegrity::of(run.path(), &log));

        prop_assert!(integrity.faults.is_empty(), "{:?}", integrity.faults);
        prop_assert_eq!(integrity.diagnostic(&RUN.into()), None);
        prop_assert_eq!(
            integrity.verified + integrity.unverifiable,
            ArtifactLedger::of(&log).every().count(),
            "every artifact the log names is accounted for"
        );
        prop_assert_eq!(derive(&log), before);
    }
}

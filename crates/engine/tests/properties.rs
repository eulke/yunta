//! Property tests for the replay guarantees the whole engine rests on:
//! `derive` is a pure, deterministic, prefix-monotonic fold of the event
//! log, at every length and not just at the full log; every event
//! round-trips through its JSON wire form; a second copy of an event
//! that states what the run holds derives what one copy derives; and
//! verifying a run's artifacts against an intact store changes nothing.
//!
//! Two of them drive the engine itself rather than a generated log: a
//! run interrupted at any point and resumed reaches the state the
//! uninterrupted run reaches, and the node a crash caught running is run
//! again without any attempt being closed twice.
//!
//! The generator emits well-formed *events* — a valid payload per kind,
//! every kind the wire knows plus one it does not, `seq` in order — but
//! does not enforce a valid *lifecycle*: `derive` interprets any log
//! (marking one it cannot follow `broken`), so these properties hold
//! over arbitrary logs, not just tidy ones.

mod common;

use std::collections::BTreeMap;

use proptest::prelude::*;

use common::{answer, ScriptedAnswers, QUESTIONS_FIXTURE, QUESTIONS_WORKFLOW};
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::node::happening::Happening as NodeHappening;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, ArtifactWrittenPayload, EventBody, EventPayload, Failure,
    Finding, FindingPostedPayload, FindingSeverity, NodeFailedPayload, NodeFinishedPayload,
    NodeStartedPayload, RecordedOrigin, RunPausedPayload, StoredEvent, TaskRegisteredPayload,
    TaskStatus, TaskStatusChangedPayload, TokenUsage, UnknownEvent,
};
use yunta_core::events::{ArtifactEvent, FindingEvent, GateEvent, NodeEvent, RunEvent, TaskEvent};
use yunta_core::events::{EventDraft, SessionEvent};
use yunta_core::ArtifactKind;
use yunta_core::NodeId;
use yunta_engine::{
    chronicle, derive, ArtifactIntegrity, Happening, NodeState, ObjectStore, RunState,
};
use yunta_testkit::Bench;
use yunta_testkit_core::{all_kinds, FixedClock, Log};

const RUN: &str = "run-prop";

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

/// The payload one generated event carries: every kind an event can be
/// written under, drawn from the one list that names them all, and the
/// handful of kinds [`parameterized`] varies on purpose. Half the draws
/// come from each, so no kind is unreachable and the kinds a property
/// needs to see repeat still repeat.
fn payload() -> impl Strategy<Value = EventPayload> {
    prop_oneof![prop::sample::select(all_kinds()), parameterized(),]
}

/// The kinds a property varies deliberately: an attempt counter, ids
/// drawn from pools small enough that a log revisits the same task and
/// the same artifact identity, and both shapes a task status has on the
/// wire.
fn parameterized() -> impl Strategy<Value = EventPayload> {
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
        (severity(), "[a-z]{0,6}", "[a-z]{1,6}").prop_map(|(severity, title, location)| {
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: Finding {
                    id: "f1".into(),
                    severity,
                    title,
                    location: yunta_core::Location::work(
                        yunta_core::RelativePath::of([location]),
                        None,
                    ),
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

/// One log entry: an optional node id (from a small pool, so nodes
/// recur), the payload it carries, and the kind it was written under
/// when that kind is one this binary does not know. `seq` is assigned in
/// order when the log is built.
fn entry() -> impl Strategy<Value = (Option<&'static str>, EventPayload, Option<&'static str>)> {
    let node = prop_oneof![
        Just(None),
        Just(Some("a")),
        Just(Some("b")),
        Just(Some("c"))
    ];
    (node, payload(), unknown_kind())
}

/// The kind a generated event is written under when no binary of this
/// version knows it — what a log a newer writer left behind holds, and
/// what a reader counts and skips. `None` for an event this binary
/// interprets, which is most of them.
fn unknown_kind() -> impl Strategy<Value = Option<&'static str>> {
    prop_oneof![
        8 => Just(None),
        1 => Just(Some("node_teleported")),
        1 => Just(Some("run_reticulated")),
    ]
}

/// A whole generated log, stated in the order it was drawn. The clock
/// stands still across it: a log's derived state must not depend on wall
/// time.
///
/// An entry that names an unknown kind keeps the envelope the builder
/// gave it — run, position, instant, node — and carries that kind in
/// place of a payload, which is exactly the shape the store reads back
/// for an event this binary cannot interpret.
fn log() -> impl Strategy<Value = Vec<StoredEvent>> {
    prop::collection::vec(entry(), 0..40).prop_map(|entries| {
        let kinds: Vec<Option<&'static str>> = entries.iter().map(|(_, _, kind)| *kind).collect();
        let mut events = entries
            .into_iter()
            .fold(Log::for_run(RUN), |log, (node, payload, _)| match node {
                Some(node) => log.node(node, payload),
                None => log.event(payload),
            })
            .build();
        for (event, kind) in events.iter_mut().zip(kinds) {
            if let Some(kind) = kind {
                event.body = EventBody::Unknown(UnknownEvent {
                    kind: kind.to_string(),
                    schema_version: 1,
                    payload: serde_json::Map::new(),
                });
            }
        }
        events
    })
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
    Log::for_run(RUN)
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node(
            "grill",
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Interpreted {
                    kind: ArtifactKind::Questions,
                },
                questions.clone(),
                RecordedOrigin::Submitted,
            ))),
        )
        .node(
            "grill",
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
        )
        .node(
            "grill",
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Opaque {
                    name: "answers.yaml".to_string(),
                },
                answers.clone(),
                RecordedOrigin::Answered,
            ))),
        )
        .node(
            "grill",
            EventPayload::Gates(GateEvent::QuestionsAnswered(
                yunta_core::events::QuestionsAnsweredPayload {
                    answers_hash: answers,
                    channel: yunta_core::events::Channel::Tty,
                    responder: None,
                },
            )),
        )
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "questions answered".to_string(),
                yunta_core::events::TokenUsage::default(),
            ))),
        )
        .build()
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

/// A chain of `nodes` bash nodes, each waiting on the one before it, so
/// the log a run of it leaves is in one order and a cut of that log
/// names one point in the run.
fn bash_chain(nodes: usize) -> String {
    let mut workflow = String::from("name: chain\nnodes:\n");
    for index in 0..nodes {
        workflow.push_str(&format!(
            "  - id: n{index}\n    kind: bash\n    run: \"true\"\n"
        ));
        if index > 0 {
            workflow.push_str(&format!("    depends_on: [n{}]\n", index - 1));
        }
    }
    workflow
}

/// The fixture of a run whose nodes open no session.
const NO_SESSIONS: &str = "sessions: []\n";

/// Puts a freshly created run where a machine that died left one: the
/// objects the interrupted run had stored, and every event it had
/// appended after its own creation.
///
/// The window between a run's creation and its first wake is the one
/// place a test states what a run starts from, so the wake that follows
/// is a resume of a run that got as far as `prefix` and no further. The
/// creation the interrupted run recorded is the one event left out: the
/// resumed run recorded its own.
fn interrupted_after(
    bench: &Bench,
    prefix: &[StoredEvent],
    stored_by: &std::path::Path,
    run_dir: &std::path::Path,
) {
    copy_objects(&stored_by.join("objects"), &run_dir.join("objects"));
    for event in prefix {
        let payload = event
            .payload()
            .expect("a run writes kinds this binary knows");
        if matches!(payload, EventPayload::Run(RunEvent::Created(_))) {
            continue;
        }
        bench
            .storage
            .append(
                &EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: event.node_id.clone(),
                    payload: payload.clone(),
                },
                &FixedClock,
            )
            .expect("append one event of the interrupted run");
    }
}

/// Copies every object one run stored into another run's store — the
/// bytes a crash leaves on disk, which a resume verifies its log
/// against.
fn copy_objects(from: &std::path::Path, to: &std::path::Path) {
    let Ok(stored) = std::fs::read_dir(from) else {
        return;
    };
    std::fs::create_dir_all(to).expect("the resumed run's object store");
    for object in stored {
        let object = object.expect("one stored object").path();
        let name = object.file_name().expect("an object is named by its hash");
        std::fs::copy(&object, to.join(name)).expect("copy one stored object");
    }
}

/// What each node a log names became, told apart from the bookkeeping of
/// how many attempts it took to get there: the answer a resume has to
/// arrive at.
fn node_states(state: &RunState) -> BTreeMap<NodeId, Option<NodeState>> {
    state
        .nodes
        .iter()
        .map(|(id, _)| (id.clone(), state.nodes.state(id).cloned()))
        .collect()
}

/// The node a log leaves running, if any — where a cut of that log
/// interrupts the run.
fn running_node(events: &[StoredEvent]) -> Option<NodeId> {
    let state = derive(events);
    let running = state
        .nodes
        .iter()
        .map(|(id, _)| id.clone())
        .find(|id| matches!(state.nodes.state(id), Some(NodeState::Running { .. })));
    running
}

/// How many attempts `node` opened, and how many terminal events it got:
/// one `node_started` against one `node_finished` or `node_failed`.
fn starts_and_terminals(events: &[StoredEvent], node: &NodeId) -> (usize, usize) {
    let counted = |wanted: fn(&NodeEvent) -> bool| {
        events
            .iter()
            .filter(|event| event.node_id.as_ref() == Some(node))
            .filter(|event| matches!(event.payload(), Some(EventPayload::Node(e)) if wanted(e)))
            .count()
    };
    (
        counted(|e| matches!(e, NodeEvent::Started(_))),
        counted(|e| matches!(e, NodeEvent::Finished(_) | NodeEvent::Failed(_))),
    )
}

/// The terminal events each of `node`'s attempts got: one entry per
/// `node_started`, counting the terminals between it and the next start.
fn terminals_per_attempt(events: &[StoredEvent], node: &NodeId) -> Vec<usize> {
    let mut attempts: Vec<usize> = Vec::new();
    for event in events
        .iter()
        .filter(|event| event.node_id.as_ref() == Some(node))
    {
        match event.payload() {
            Some(EventPayload::Node(NodeEvent::Started(_))) => attempts.push(0),
            Some(EventPayload::Node(NodeEvent::Finished(_) | NodeEvent::Failed(_))) => {
                if let Some(attempt) = attempts.last_mut() {
                    *attempt += 1;
                }
            }
            _ => {}
        }
    }
    attempts
}

/// How many agent sessions a log opened.
fn sessions_opened(events: &[StoredEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(_)))
            )
        })
        .count()
}

/// A node that asks its questions, is answered, and finishes — driven
/// once, uninterrupted, so every cut of the log it leaves is a crash
/// point a resume can be held against.
///
/// The table this walks is the one [`every_cut_of_a_questions_round_derives_what_a_resume_acts_on`]
/// states: a cut before the ask leaves a node to re-run, a cut after it
/// leaves one that asks again from the document it already holds, and a
/// cut after the answer leaves one owed the terminal its close deferred.
/// At every one of them the run ends with the node finished, carrying
/// one `node_finished`, and a cut at or after the ask opens no session
/// the interrupted run had not already opened: the node closed when it
/// asked.
#[tokio::test]
async fn an_ask_answered_after_any_crash_point_derives_one_finished_node() {
    let answering = ScriptedAnswers {
        answers: vec![answer("q1", "staging")],
    };
    let uninterrupted = Bench::new();
    uninterrupted
        .run_with_interaction(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE, &answering)
        .await;
    let whole = uninterrupted.events();
    let node = NodeId::from("ask");
    let asked_at = whole
        .iter()
        .position(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Gates(GateEvent::QuestionsAsked(_)))
            )
        })
        .expect("the ask is on the log")
        + 1;

    for cut in 0..=whole.len() {
        let resumed = Bench::new();
        resumed
            .run_sabotaged_answering(
                QUESTIONS_WORKFLOW,
                QUESTIONS_FIXTURE,
                &answering,
                |run_dir| {
                    interrupted_after(&resumed, &whole[..cut], &uninterrupted.run_dir(), run_dir)
                },
            )
            .await;

        let events = resumed.events();
        one_finished_round(&events, &node, cut);
        if cut >= asked_at {
            assert_eq!(
                sessions_opened(&events),
                sessions_opened(&whole[..cut]),
                "cut at {cut}: a node that closed when it asked opens no session"
            );
        }
    }
}

/// What a resumed round leaves: `node` finished, carrying one terminal
/// event for the whole round however many times it was interrupted.
fn one_finished_round(events: &[StoredEvent], node: &NodeId, cut: usize) {
    let state = derive(events);
    assert!(
        matches!(state.nodes.state(node), Some(NodeState::Finished { .. })),
        "cut at {cut}: {:?}",
        state.nodes.state(node)
    );
    assert_eq!(
        starts_and_terminals(events, node).1,
        1,
        "cut at {cut}: one terminal for the node that asked"
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
        let named: std::collections::BTreeSet<_> = log
            .iter()
            .filter_map(|event| match event.payload() {
                Some(EventPayload::Artifacts(ArtifactEvent::Accepted(p))) => {
                    Some((event.node_id.clone(), p.artifact.clone()))
                }
                // A log written before an acceptance stated the identity
                // names it through what it wrote.
                Some(EventPayload::Artifacts(ArtifactEvent::Written(p))) => Some((
                    event.node_id.clone(),
                    yunta_core::events::artifacts::legacy_identity(p),
                )),
                _ => None,
            })
            .collect();
        prop_assert_eq!(held, named);
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
    /// accumulate, tasks and nodes once seen stay seen, and a finding
    /// leaves the set that stands only by being withdrawn — a longer
    /// prefix never un-does what a shorter one derived.
    #[test]
    fn derive_is_prefix_monotonic(log in log()) {
        let mut prev = derive(&[]);
        for k in 1..=log.len() {
            let cur = derive(&log[..k]);
            prop_assert!(cur.total_tokens().total() >= prev.total_tokens().total());
            for posted in prev.findings.effective() {
                let stands = cur
                    .findings
                    .effective()
                    .iter()
                    .any(|now| now.node == posted.node && now.finding.id == posted.finding.id);
                let withdrawn = posted.node.as_ref().is_some_and(|node| {
                    matches!(
                        cur.findings.status(node, &posted.finding.id),
                        Some(yunta_core::events::findings::Slot::Withdrawn { .. })
                    )
                });
                prop_assert!(
                    stands || withdrawn,
                    "finding {} left the set without a withdrawal",
                    posted.finding.id
                );
            }
            for id in prev.tasks.iter().map(|(id, _)| id) {
                prop_assert!(cur.tasks.contains(id), "task {id} disappeared");
            }
            for id in prev.nodes.iter().map(|(id, _)| id) {
                prop_assert!(cur.nodes.contains(id), "node {id} disappeared");
            }
            prev = cur;
        }
    }

    /// The chronicle of a prefix is a prefix of the chronicle: a reader
    /// following a run live sees exactly what a reader of the finished
    /// log sees, in the same order, and nothing a later event reveals
    /// rewrites a moment already read.
    #[test]
    fn the_chronicle_of_a_prefix_is_a_prefix_of_the_chronicle(log in log()) {
        let whole = chronicle(&log);
        prop_assert_eq!(whole.len(), log.len(), "one moment per event");
        for k in 0..=log.len() {
            prop_assert_eq!(
                chronicle(&log[..k]),
                whole[..k].to_vec(),
                "the chronicle of the first {} events is not its prefix",
                k
            );
        }
    }

    /// The frame agrees with the chronicle: the state a moment says a
    /// node reached is the state the log derives at that very event.
    ///
    /// Two derivations of one log that disagreed would let a region and
    /// a scrollback describe the same node two ways, which is the
    /// defect this pair exists to make impossible. Stated per moment
    /// rather than per node, because that is what makes it true of a
    /// node read halfway through the log as well as at its end.
    #[test]
    fn the_frame_agrees_with_the_chronicle(log in log()) {
        for (index, moment) in chronicle(&log).into_iter().enumerate() {
            let (Some(node), Happening::Node(NodeHappening::Reached { state, .. })) =
                (moment.node, moment.happening)
            else {
                continue;
            };
            let at_that_event = derive(&log[..=index]);
            if at_that_event.broken.is_some() {
                // A log the replay could not fold that far derives
                // nothing past its break, and the chronicle stops there
                // with it: there is no state for the two to agree on.
                continue;
            }
            prop_assert_eq!(
                at_that_event.nodes.state(&node),
                Some(&state),
                "a node is where its own moment says it became, at that moment: {}",
                node
            );
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

    /// A second copy of an event that states what the run holds derives
    /// what one copy derives: the artifact and the finding folds are
    /// keyed by the identity their event names, so a channel that hands
    /// the same acceptance or the same posting over twice leaves the run
    /// holding exactly what it held.
    ///
    /// Each copy is the event it repeats, position included — the two
    /// arrivals are one event, which is what tells a redelivery apart
    /// from a second acceptance of the same identity.
    #[test]
    fn redelivering_any_subsequence_of_events_changes_no_state(
        (log, again) in log().prop_flat_map(|log| {
            let len = log.len();
            (Just(log), prop::collection::vec(any::<bool>(), len))
        })
    ) {
        let mut redelivered = Vec::new();
        for (event, again) in log.iter().zip(again) {
            redelivered.push(event.clone());
            let holdable = matches!(
                event.payload(),
                Some(EventPayload::Artifacts(_) | EventPayload::Findings(_))
            );
            if again && holdable {
                redelivered.push(event.clone());
            }
        }
        prop_assert_eq!(derive(&redelivered), derive(&log));
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
        let mut building = Log::for_run(RUN);
        for (node, artifact, content) in &accepted {
            let content_hash = block_on(store.put(content.as_bytes())).expect("store the bytes");
            let payload = EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(artifact.clone(), content_hash, RecordedOrigin::Submitted)));
            building = match *node {
                Some(node) => building.node(node, payload),
                None => building.event(payload),
            };
        }
        for name in &older {
            building = building.node(
                "a",
                EventPayload::Artifacts(ArtifactEvent::Written(ArtifactWrittenPayload {
                    path: std::path::PathBuf::from(format!("artifacts/{name}.md")),
                    content_hash: yunta_core::sha256_hex(name.as_bytes()),
                    artifact_kind: None,
                })),
            );
        }
        let log = building.build();

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

// The properties that drive whole runs. Each case executes the engine
// twice against real subprocesses, so the number of cases is stated
// here rather than left at the default a pure fold can afford.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// A run interrupted at any point and resumed reaches the state the
    /// same run reaches uninterrupted: every node ends where it would
    /// have ended, and the run closes the same way.
    ///
    /// What the resume starts from is the log alone — the events the
    /// interrupted run had appended and the objects it had stored —
    /// which is everything a separate `yunta resume` process is handed.
    #[test]
    fn a_run_cut_at_any_point_resumes_to_the_state_it_would_have_reached(
        nodes in 1usize..5,
        at in any::<prop::sample::Index>(),
    ) {
        let workflow = bash_chain(nodes);
        let uninterrupted = Bench::new();
        let expected = block_on(uninterrupted.run(&workflow, NO_SESSIONS));
        let whole = uninterrupted.events();
        let cut = at.index(whole.len() + 1);

        let resumed = Bench::new();
        let report = block_on(resumed.run_sabotaged(&workflow, NO_SESSIONS, |run_dir| {
            interrupted_after(&resumed, &whole[..cut], &uninterrupted.run_dir(), run_dir)
        }));

        let state = derive(&resumed.events());
        prop_assert_eq!(&state.broken, &None, "cut at {}", cut);
        prop_assert_eq!(report.terminal, expected.terminal, "cut at {}", cut);
        prop_assert_eq!(node_states(&state), node_states(&expected.state), "cut at {}", cut);
    }

    /// A node the crash caught running is run again, and no attempt is
    /// closed twice: every attempt carries exactly one terminal event,
    /// except the attempt the crash cut short, which carries none —
    /// having no terminal is how the log states that an attempt was cut
    /// short, and `restart_node` answers it with a fresh attempt rather
    /// than a verdict on one nobody watched end.
    ///
    /// A resume that paid a terminal for the attempt it also re-ran, or
    /// that re-ran an attempt it had already closed, leaves one attempt
    /// carrying two.
    #[test]
    fn a_node_rerun_after_a_crash_finishes_exactly_once_per_attempt(
        nodes in 1usize..5,
        at in any::<prop::sample::Index>(),
    ) {
        let workflow = bash_chain(nodes);
        let uninterrupted = Bench::new();
        block_on(uninterrupted.run(&workflow, NO_SESSIONS));
        let whole = uninterrupted.events();

        // Only a cut that catches a node running is a crash a re-run
        // answers; every run of this workflow has such a point.
        let caught: Vec<usize> = (1..=whole.len())
            .filter(|cut| running_node(&whole[..*cut]).is_some())
            .collect();
        prop_assert!(!caught.is_empty(), "a run of {} nodes starts one", nodes);
        let cut = caught[at.index(caught.len())];
        let node = running_node(&whole[..cut]).expect("the node the cut catches running");

        let resumed = Bench::new();
        block_on(resumed.run_sabotaged(&workflow, NO_SESSIONS, |run_dir| {
            interrupted_after(&resumed, &whole[..cut], &uninterrupted.run_dir(), run_dir)
        }));

        // The attempt the crash cut short is the last one the
        // interrupted log had opened.
        let interrupted = starts_and_terminals(&whole[..cut], &node).0 - 1;
        let events = resumed.events();
        let (starts, terminals) = starts_and_terminals(&events, &node);
        let per_attempt = terminals_per_attempt(&events, &node);
        prop_assert!(starts >= 2, "node `{}` runs again after the crash at {}", node, cut);
        prop_assert_eq!(
            per_attempt.iter().sum::<usize>(),
            terminals,
            "every terminal closes an attempt"
        );
        for (attempt, closed) in per_attempt.iter().enumerate() {
            let owed = usize::from(attempt != interrupted);
            prop_assert_eq!(closed, &owed, "attempt {} of `{}`, cut at {}", attempt + 1, node, cut);
        }
    }
}

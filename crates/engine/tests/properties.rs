//! Property tests the engine itself is held to: verifying a run's artifacts
//! against an intact store finds nothing and changes nothing, over generated
//! logs; and two that drive the engine rather than a generated log: a run
//! interrupted at any point and resumed reaches the state the uninterrupted
//! run reaches, and the node a crash caught running is run again without
//! any attempt being closed twice.

mod common;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use proptest::prelude::*;

use common::{answer, ScriptedAnswers, QUESTIONS_FIXTURE, QUESTIONS_WORKFLOW};
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactWrittenPayload, EventPayload, NodeState, RecordedOrigin,
    StoredEvent,
};
use yunta_core::events::{ArtifactEvent, EventDraft, GateEvent, NodeEvent, RunEvent, SessionEvent};
use yunta_core::NodeId;
use yunta_engine::{ArtifactIntegrity, ObjectStore, RunReport};
use yunta_replay::RunState;
use yunta_testkit::Bench;
use yunta_testkit_core::arbitrary::{accepted_artifacts, RUN};
use yunta_testkit_core::{FixedClock, Log};

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
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// What a run of a chain left when nothing stopped it: the bench it ran
/// on, whose store a resume copies objects from, its log, and how it
/// ended.
struct Uninterrupted {
    bench: Bench,
    whole: Vec<StoredEvent>,
    report: RunReport,
}

thread_local! {
    static UNINTERRUPTED: RefCell<BTreeMap<usize, Rc<Uninterrupted>>> = RefCell::default();
}

/// The uninterrupted run of a chain of `nodes`, run once per thread. A
/// bench runs on a fixed clock and hands out ids in sequence, so the same
/// chain leaves the same log every time — all but its creation, which
/// names the commit the bench's repository started from and which a
/// resume never replays — and every case that cuts it can cut the one
/// copy.
fn uninterrupted(nodes: usize) -> Rc<Uninterrupted> {
    UNINTERRUPTED.with(|runs| {
        Rc::clone(runs.borrow_mut().entry(nodes).or_insert_with(|| {
            let bench = Bench::new();
            let report = block_on(bench.run(&bash_chain(nodes), NO_SESSIONS));
            let whole = bench.events();
            Rc::new(Uninterrupted {
                bench,
                whole,
                report,
            })
        }))
    })
}

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
                &EventDraft::new(bench.run_id.clone(), event.node_id.clone(), payload.clone())
                    .expect("a draft names its subject"),
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
    let state = RunState::replay(events);
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
    let state = RunState::replay(events);
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

        let before = RunState::replay(&log);
        let integrity = block_on(ArtifactIntegrity::of(run.path(), &log));

        prop_assert!(integrity.faults.is_empty(), "{:?}", integrity.faults);
        prop_assert_eq!(integrity.diagnostic(&RUN.into()), None);
        prop_assert_eq!(
            integrity.verified + integrity.unverifiable,
            ArtifactLedger::of(&log).every().count(),
            "every artifact the log names is accounted for"
        );
        prop_assert_eq!(RunState::replay(&log), before);
    }
}

// The properties that drive whole runs. Each case resumes a run against
// real subprocesses, so the number of cases is stated here rather than
// left at the default a pure fold can afford.
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
        let uninterrupted = uninterrupted(nodes);
        let Uninterrupted { whole, report: expected, .. } = uninterrupted.as_ref();
        let cut = at.index(whole.len() + 1);

        let resumed = Bench::new();
        let report = block_on(resumed.run_sabotaged(&workflow, NO_SESSIONS, |run_dir| {
            interrupted_after(&resumed, &whole[..cut], &uninterrupted.bench.run_dir(), run_dir)
        }));

        let state = RunState::replay(&resumed.events());
        prop_assert_eq!(&state.broken, &None, "cut at {}", cut);
        prop_assert_eq!(&report.terminal, &expected.terminal, "cut at {}", cut);
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
        let uninterrupted = uninterrupted(nodes);
        let whole = &uninterrupted.whole;

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
            interrupted_after(&resumed, &whole[..cut], &uninterrupted.bench.run_dir(), run_dir)
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

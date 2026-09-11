//! The display-only observation boundary, held to the log it mirrors.
//!
//! The engine appends events from several places — the scheduler's own
//! `emit`, a live session's audit trail, the run-tools listener an agent
//! posts findings through — and a surface drawing the run is only
//! correct if it sees all of them. These tests compare what an observer
//! recorded against what the run actually wrote, so an append site added
//! without feeding the boundary fails here instead of silently going
//! missing from a live view.

use yunta_core::events::EventPayload;
use yunta_core::{RunId, Seq};
use yunta_engine::RunTerminal;
use yunta_testkit::{Bench, RecordingObserver};

const ONE_SESSION_AND_A_CHECK: &str = r#"
name: observed
nodes:
  - id: think
    kind: prompt
    runner: executor
    prompt: "Do the thing."
  - id: check
    kind: bash
    run: "true"
    depends_on: [think]
"#;

const COMPLETES: &str = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;

/// A session that calls `yunta_post_finding` as a real MCP client of the
/// engine's own per-run listener — the append site reached over the wire
/// rather than from the scheduler's task.
const POSTS_A_FINDING: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - { type: run_tool, tool: yunta_post_finding, arguments: { id: seen-it, severity: minor, title: "a smell", location: "src/x.rs", detail: "spotted mid-session" } }
    outcome: { type: completed, summary: "done" }
"#;

/// `(run, position, kind)` — the identity of an event on the log,
/// independent of its payload, which is what a frame must agree with.
type Landmark = (RunId, Seq, String);

/// Driven by the fixture that reaches all three append sites in one run,
/// so the comparison covers the whole boundary rather than the
/// scheduler's share of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_event_the_run_appends_reaches_the_observer() {
    let recorder = RecordingObserver::new();
    let bench = Bench::new().with_observer(recorder.clone());

    let (terminal, state) = bench.run(ONE_SESSION_AND_A_CHECK, POSTS_A_FINDING).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let on_the_log: Vec<Landmark> = bench
        .events()
        .into_iter()
        .filter(|event| event.body.kind_name() != "run_created")
        .map(|event| {
            let kind = event.body.kind_name().to_string();
            (event.run_id, event.seq, kind)
        })
        .collect();
    let observed: Vec<Landmark> = recorder
        .frames()
        .into_iter()
        .map(|frame| (frame.run_id, frame.seq, frame.kind.to_string()))
        .collect();

    // The comparison below is only as wide as the run that feeds it, so
    // the run is pinned to reach all three append sites: a session's
    // `agent_session_opened`, the listener's `finding_posted`, and the
    // scheduler's own node and run events around them.
    for kind in ["agent_session_opened", "finding_posted", "node_finished"] {
        assert!(
            on_the_log.iter().any(|(_, _, on_log)| on_log == kind),
            "the fixture must drive every append site into the log, and wrote no {kind}: \
             {on_the_log:?}"
        );
    }
    assert_eq!(
        observed, on_the_log,
        "every event a run appends while this process executes it must reach its observer, \
         in the order the log carries them — the scheduler's own `emit`, the live \
         session's audit trail and the run-tools listener alike, all three of which this \
         run drives. `create_run`'s `run_created` is the one exception: it is written \
         before any execution context, and so any observer, exists, and a caller that \
         draws the run reads it as its seed. A difference here means an append site \
         writes to the log without going through `observer::append_observed`."
    );
}

#[tokio::test]
async fn a_session_audit_event_reaches_the_observer() {
    let recorder = RecordingObserver::new();
    let bench = Bench::new().with_observer(recorder.clone());

    let (terminal, state) = bench.run(ONE_SESSION_AND_A_CHECK, COMPLETES).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    // `agent_session_opened` is written by `RunCtx`'s `SessionObserver`
    // impl and nowhere else, so its frame proves that path feeds the
    // boundary too — not just the scheduler's own `emit`.
    let opened: Vec<_> = recorder
        .frames()
        .into_iter()
        .filter(|frame| matches!(frame.payload, EventPayload::AgentSessionOpened(_)))
        .collect();
    assert_eq!(
        opened.len(),
        1,
        "the live session's audit trail must reach the observer, got kinds: {:?}",
        recorder.kinds()
    );
    assert_eq!(
        opened
            .first()
            .and_then(|frame| frame.node_id.as_ref())
            .map(yunta_core::NodeId::as_str),
        Some("think"),
        "a session's frames name the node whose session it is"
    );
}

const REVIEWS: &str = r#"
name: posting
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review it."
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_posted_finding_reaches_the_observer() {
    let recorder = RecordingObserver::new();
    let bench = Bench::new().with_observer(recorder.clone());

    let (terminal, state) = bench.run(REVIEWS, POSTS_A_FINDING).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let posted: Vec<_> = recorder
        .frames()
        .into_iter()
        .filter_map(|frame| match (&frame.payload, &frame.node_id) {
            (EventPayload::FindingPosted(p), Some(node)) => {
                Some((node.to_string(), p.finding.id.to_string()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        posted,
        vec![("review".to_string(), "seen-it".to_string())],
        "what an agent posts through the run-tools listener must reach the observer as it \
         lands, got kinds: {:?}",
        recorder.kinds()
    );
}

const CHILD: &str = r#"
name: child-wf
nodes:
  - id: work
    kind: bash
    run: "true"
"#;

const PARENT: &str = r#"
name: parent
nodes:
  - id: feat
    kind: workflow
    use: child-wf
"#;

#[tokio::test]
async fn a_child_run_reports_under_its_own_run_id() {
    let recorder = RecordingObserver::new();
    let bench = Bench::new()
        .with_workflow("child-wf", CHILD)
        .with_observer(recorder.clone());

    let (terminal, state) = bench.run(PARENT, "sessions: []\n").await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let frames = recorder.frames();
    let child_id = frames
        .iter()
        .find_map(|frame| match &frame.payload {
            EventPayload::ChildRunCreated(p) => Some(p.child_run_id.clone()),
            _ => None,
        })
        .expect("the parent records the child run it gives birth to");
    assert_ne!(
        child_id, bench.run_id,
        "a `kind: workflow` child is a run of its own"
    );

    // One observer serves the whole invocation, and each run's frames
    // carry that run's own id — the child emits through its own context.
    assert!(
        !recorder.for_run(&bench.run_id).is_empty(),
        "the parent's own frames must be there"
    );
    assert!(
        recorder
            .for_run(&child_id)
            .iter()
            .any(|frame| matches!(frame.payload, EventPayload::RunFinished(_))),
        "the child's frames must reach the same observer, under the child's run id"
    );

    let link_at = frames
        .iter()
        .position(|frame| matches!(frame.payload, EventPayload::ChildRunCreated(_)))
        .expect("the link frame");
    let first_child_at = frames
        .iter()
        .position(|frame| frame.run_id == child_id)
        .expect("a frame from the child");
    assert!(
        link_at < first_child_at,
        "the parent's link frame arrives before any of the child's, so a surface learns \
         the child exists before it has the child's events to draw"
    );
}

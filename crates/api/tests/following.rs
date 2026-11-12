//! Following a run from a process that does not drive it, through the API: over a log
//! and a registry a test writes, what each look at the run hands back, and where it
//! ends.

mod bench_project;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_api::{Error, Following, Project, RunNews, Stop};
use yunta_core::events::{
    EventDraft, EventPayload, HostSuspendedPayload, RunCreatedPayload, RunEvent,
    RunFinishedPayload, RunMetrics, TerminalState, TokenUsage,
};
use yunta_core::persisted::PersistedDoc;
use yunta_core::port::Said;
use yunta_core::{Clock, NodeId, Pid, RunId, Seq, SessionId};
use yunta_engine::lock::SystemProbe;
use yunta_engine::said::{Socket, Voices};
use yunta_engine::ProcessRegistry;
use yunta_testkit::{Bench, SourceLog};
use yunta_testkit_core::FixedClock;

use bench_project::project_of;

/// One node, so a run of it has a frame to stand in.
const WORKFLOW: &str = "name: followed\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

/// A bench's world read as a project: its runs root and its log.
struct World {
    bench: Bench,
    project: Project,
}

impl World {
    fn new() -> World {
        let bench = Bench::with_run_id("followed");
        let project = project_of(&bench);
        World { bench, project }
    }

    fn clock(&self) -> Arc<dyn Clock> {
        Arc::new(FixedClock)
    }

    /// Run `run`'s directory, with the manifest it froze in it.
    async fn dir_of(&self, run: &RunId) -> PathBuf {
        let dir = self.bench.runs_root.join(run.as_str());
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = self.bench.manifest_for(WORKFLOW, "").await;
        let bytes = PersistedDoc::of(manifest).write().unwrap();
        std::fs::write(yunta_engine::run_dir::manifest_path(&dir), bytes).unwrap();
        std::fs::create_dir_all(yunta_engine::registry_path(&dir).parent().unwrap()).unwrap();
        dir
    }

    /// Run `run`, born on the log with its manifest beside it, and nothing since.
    async fn born(&self, run: &str) -> (RunId, PathBuf) {
        let run = RunId::from(run);
        let dir = self.dir_of(&run).await;
        SourceLog::open(&self.bench.storage, &run, self.clock());
        (run, dir)
    }

    /// Appends `payload` to `run`'s log.
    fn record(&self, run: &RunId, payload: EventPayload) -> Seq {
        let draft = EventDraft::new(run.clone(), None, payload).unwrap();
        self.bench.storage.append(&draft, &FixedClock).unwrap()
    }

    /// Run `successor`, born on the log as promoted from `from`, its manifest beside it.
    async fn successor_of(&self, from: &RunId, successor: &str) -> RunId {
        let successor = RunId::from(successor);
        self.dir_of(&successor).await;
        let created = RunCreatedPayload {
            checkout: None,
            manifest_hash: yunta_core::sha256_hex(successor.as_str().as_bytes()),
            inputs: Default::default(),
            mode: Default::default(),
            promoted_from: Some(from.clone()),
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: yunta_core::sha256_hex(b"base").as_str().into(),
            environment: None,
            left_out: Vec::new(),
            opens_on_base: false,
        };
        self.record(&successor, EventPayload::Run(RunEvent::Created(created)));
        successor
    }
}

/// A registry naming this test's own process as the engine driving the run in `dir`:
/// alive for as long as the handle is held.
fn driven_by_this_process(dir: &Path) -> ProcessRegistry {
    ProcessRegistry::create(dir, Pid::current(), &SystemProbe, &FixedClock).unwrap()
}

/// A registry the way an engine that died leaves one: naming a process that ended.
fn left_by_a_dead_engine(dir: &Path) {
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let pid = Pid::try_from(ended.id()).unwrap();
    ended.wait().unwrap();
    std::mem::forget(ProcessRegistry::create(dir, pid, &SystemProbe, &FixedClock).unwrap());
}

/// What one of `run`'s sessions says, `text`.
fn said(run: &RunId, text: &str) -> Said {
    Said {
        run_id: run.clone(),
        node_id: NodeId::from("only"),
        session_id: SessionId::from_static("session-1"),
        task_id: None,
        seq: Seq::from(1),
        at: chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap_or_default(),
        text: text.to_string(),
        cut: false,
    }
}

/// The next news of `following`, which a test expects before long.
async fn next(following: &mut Following) -> Option<RunNews> {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        following.next(&Stop::new()),
    )
    .await
    .expect("news within ten seconds")
    .expect("following reads the run")
}

#[tokio::test]
async fn what_came_past_the_last_event_read_is_handed_back() {
    let world = World::new();
    let (run, dir) = world.born("moving").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.project.follow(&run).await.unwrap();
    assert_eq!(
        following.followed().events.len(),
        1,
        "the run's birth is read whole"
    );

    let seq = world.record(
        &run,
        EventPayload::Run(RunEvent::HostSuspended(HostSuspendedPayload::slept(
            std::time::Duration::from_secs(3),
        ))),
    );
    let Some(RunNews::Events(events)) = next(&mut following).await else {
        panic!("the event appended comes back");
    };
    let seqs: Vec<Seq> = events.iter().map(|event| event.seq).collect();
    assert_eq!(seqs, vec![seq]);
    assert_eq!(following.followed().events.len(), 2, "what came is read on");
}

#[tokio::test]
async fn a_tripped_stop_ends_following_with_the_stop() {
    let world = World::new();
    let (run, dir) = world.born("moving").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.project.follow(&run).await.unwrap();
    let stop = Stop::new();
    stop.stop();
    assert!(matches!(following.next(&stop).await, Err(Error::Stopped)));
}

#[tokio::test]
async fn a_run_whose_engine_is_gone_stalls_and_then_hands_back_nothing() {
    let world = World::new();
    let (run, dir) = world.born("orphaned").await;
    left_by_a_dead_engine(&dir);
    let mut following = world.project.follow(&run).await.unwrap();
    assert!(matches!(next(&mut following).await, Some(RunNews::Stalled)));
    assert!(next(&mut following).await.is_none());
}

#[tokio::test]
async fn a_run_that_closes_promoted_is_followed_onto_its_successor() {
    let world = World::new();
    let (run, dir) = world.born("promoted").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.project.follow(&run).await.unwrap();

    let closed = RunFinishedPayload::closed(
        TerminalState::Promoted,
        RunMetrics::of(TokenUsage::default(), 0),
    );
    world.record(&run, EventPayload::Run(RunEvent::Finished(closed)));
    let successor = world.successor_of(&run, "successor").await;

    assert!(matches!(
        next(&mut following).await,
        Some(RunNews::Events(_))
    ));
    let Some(RunNews::Successor(next_run)) = next(&mut following).await else {
        panic!("the successor is followed on");
    };
    assert_eq!(next_run.run, successor);
    assert_eq!(next_run.events.len(), 1, "the successor's log as it stands");
    assert_eq!(following.followed().run, successor);
}

/// A copy of the run followed, taken as following starts and handed each piece of news,
/// stands where the following does: onto the successor, with its whole log.
#[tokio::test]
async fn a_followed_run_handed_each_news_stands_where_the_following_does() {
    let world = World::new();
    let (run, dir) = world.born("copied").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.project.follow(&run).await.unwrap();
    let mut kept = following.followed().clone();

    let closed = RunFinishedPayload::closed(
        TerminalState::Promoted,
        RunMetrics::of(TokenUsage::default(), 0),
    );
    world.record(&run, EventPayload::Run(RunEvent::Finished(closed)));
    let successor = world.successor_of(&run, "copied-on").await;
    while kept.run != successor {
        let news = next(&mut following)
            .await
            .expect("news until the successor");
        kept.took(&news);
    }

    let followed = following.followed();
    assert_eq!(kept.run, followed.run);
    assert_eq!(kept.events.len(), followed.events.len());
    assert_eq!(kept.engine, followed.engine);
}

#[tokio::test]
async fn what_a_session_says_on_the_runs_socket_is_handed_back() {
    let world = World::new();
    let (run, dir) = world.born("speaking").await;
    let engine = driven_by_this_process(&dir);
    let socket_dir = tempfile::Builder::new()
        .prefix("w")
        .tempdir_in("/tmp")
        .unwrap();
    let voices = Arc::new(Voices::new());
    let path = socket_dir.path().join("words.sock");
    let socket = Socket::bind(&path, Arc::clone(&voices), yunta_core::process::user::uid());
    engine.serve_words(socket.unwrap());
    let mut following = world.project.follow(&run).await.unwrap();

    voices.hand(said(&run, "reading the plan"));
    let Some(RunNews::Said(heard)) = next(&mut following).await else {
        panic!("what was said comes back");
    };
    assert_eq!(heard.text, "reading the plan");
}

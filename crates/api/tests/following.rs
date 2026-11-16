//! Following a run from a process that does not drive it, through the API: over a log
//! and a registry a test writes, what each look at the run hands back, and where it
//! ends.

#[path = "following/awaited.rs"]
mod awaited;
mod bench_project;
#[path = "following/documents.rs"]
mod documents;
#[path = "following/field.rs"]
mod field;
#[path = "following/kept_logs.rs"]
mod kept_logs;
#[path = "following/said.rs"]
mod said;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use surco::words::Standing;
use yunta_api::{Error, FollowNews, Followed, FollowedRun, Following, Project, RunNews, Stop};
use yunta_core::events::{
    EventDraft, EventPayload, HostSuspendedPayload, PauseReason, RunCreatedPayload, RunEvent,
    RunFinishedPayload, RunMetrics, RunPausedPayload, RunResumedPayload, StoredEvent,
    TerminalState, TokenUsage,
};
use yunta_core::persisted::PersistedDoc;
use yunta_core::{Clock, Pid, RunId, Seq};
use yunta_engine::lock::SystemProbe;
use yunta_engine::ProcessRegistry;
use yunta_replay::frame::RunFrame;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;
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

    /// Run `run`'s directory, with the manifest it froze of `workflow` in it.
    async fn dir_of(&self, run: &RunId, workflow: &str) -> PathBuf {
        let dir = self.bench.runs_root.join(run.as_str());
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = self.bench.manifest_for(workflow, "").await;
        let bytes = PersistedDoc::of(manifest).write().unwrap();
        std::fs::write(yunta_engine::run_dir::manifest_path(&dir), bytes).unwrap();
        std::fs::create_dir_all(yunta_engine::registry_path(&dir).parent().unwrap()).unwrap();
        dir
    }

    /// Run `run`, born on the log with its manifest beside it, and nothing since.
    async fn born(&self, run: &str) -> (RunId, PathBuf) {
        let run = RunId::from(run);
        let dir = self.dir_of(&run, WORKFLOW).await;
        SourceLog::open(&self.bench.storage, &run, self.clock());
        (run, dir)
    }

    /// Appends `payload` to `run`'s log.
    fn record(&self, run: &RunId, payload: EventPayload) -> Seq {
        let draft = EventDraft::new(run.clone(), None, payload).unwrap();
        self.bench.storage.append(&draft, &FixedClock).unwrap()
    }

    /// Appends to `run`'s log an event that moves nothing but the run's clock.
    fn slept(&self, run: &RunId) -> Seq {
        self.record(
            run,
            EventPayload::Run(RunEvent::HostSuspended(HostSuspendedPayload::slept(
                Duration::from_secs(3),
            ))),
        )
    }

    /// Run `run`'s whole log as the store holds it.
    fn logged(&self, run: &RunId) -> Vec<StoredEvent> {
        self.bench.storage.events_for_run(run).unwrap()
    }

    /// `run` followed from here.
    async fn follow(&self, run: &RunId) -> Following {
        let followed = Followed {
            field: None,
            run: Some(run.clone()),
        };
        self.project.follow(followed).await.unwrap()
    }

    /// `kept`'s frame at the world's clock.
    fn frame(&self, kept: &FollowedRun) -> RunFrame {
        kept.frame(self.clock().now())
    }

    /// Run `successor`, born on the log as promoted from `from`, its manifest beside it.
    async fn successor_of(&self, from: &RunId, successor: &str) -> RunId {
        let successor = RunId::from(successor);
        self.dir_of(&successor, WORKFLOW).await;
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

/// The run's close for a successor in a later mode.
fn closed_promoted() -> EventPayload {
    let closed = RunFinishedPayload::closed(
        TerminalState::Promoted,
        RunMetrics::of(TokenUsage::default(), 0),
    );
    EventPayload::Run(RunEvent::Finished(closed))
}

/// The seqs of `events`, in their order.
fn seqs(events: &[StoredEvent]) -> Vec<Seq> {
    events.iter().map(|event| event.seq).collect()
}

/// The next news of `following`, which a test expects before long.
async fn next(following: &mut Following) -> FollowNews {
    tokio::time::timeout(Duration::from_secs(10), following.next(&Stop::new()))
        .await
        .expect("news within ten seconds")
        .expect("following reads the run")
}

/// The run `following` hands first, read whole, and every event of its log from its
/// first.
async fn whole(following: &mut Following) -> (FollowedRun, Vec<StoredEvent>) {
    match next(following).await {
        FollowNews::Whole { run, events } => (*run, events),
        other => panic!("the run read whole comes first, and came {other:?}"),
    }
}

/// The next news of the run `kept` holds, folded into it.
async fn next_of(following: &mut Following, kept: &mut FollowedRun) -> RunNews {
    match next(following).await {
        FollowNews::Run { run, news } => {
            assert_eq!(run, kept.run, "news of the run held");
            kept.took(&news);
            news
        }
        other => panic!("news of the run held, and came {other:?}"),
    }
}

#[tokio::test]
async fn what_came_past_the_last_event_read_is_handed_back() {
    let world = World::new();
    let (run, dir) = world.born("moving").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    let (mut kept, events) = whole(&mut following).await;
    assert_eq!(events.len(), 1, "the run's birth is read whole");

    let seq = world.slept(&run);
    let RunNews::Events(events) = next_of(&mut following, &mut kept).await else {
        panic!("the event appended comes back");
    };
    assert_eq!(seqs(&events), vec![seq]);
    assert_eq!(kept.state.head(), Some(seq), "what came is read on");
}

/// The run read whole arrives first, with every event of its log from its first, and
/// what comes after arrives as a batch of only what came past it.
#[tokio::test]
async fn the_run_read_whole_is_handed_every_event_from_its_first_once() {
    let world = World::new();
    let (run, dir) = world.born("whole").await;
    let _engine = driven_by_this_process(&dir);
    world.slept(&run);
    world.slept(&run);
    let mut following = world.follow(&run).await;

    let (mut kept, events) = whole(&mut following).await;
    let logged = world.logged(&run);
    assert_eq!(events, logged, "every event of the log, from its first");
    assert_eq!(kept.state, RunState::replay(&logged), "the run at its head");
    assert_eq!(kept.state.head(), logged.last().map(|event| event.seq));

    let came = world.slept(&run);
    let RunNews::Events(batch) = next_of(&mut following, &mut kept).await else {
        panic!("what came past the run read whole comes back");
    };
    assert_eq!(seqs(&batch), vec![came], "only what came past, once");
    assert_eq!(kept.state, RunState::replay(&world.logged(&run)));
}

/// Another run asked for is read whole from its first event, and the run let go hands
/// nothing more.
#[tokio::test]
async fn refollow_onto_another_run_hands_it_whole() {
    let world = World::new();
    let (first, dir) = world.born("first").await;
    let _first_engine = driven_by_this_process(&dir);
    let (second, dir) = world.born("second").await;
    let _second_engine = driven_by_this_process(&dir);
    world.slept(&second);
    let mut following = world.follow(&first).await;
    let (kept, _) = whole(&mut following).await;
    assert_eq!(kept.run, first);

    following.refollow(Followed {
        field: None,
        run: Some(second.clone()),
    });
    let (mut kept, events) = whole(&mut following).await;
    assert_eq!(kept.run, second);
    assert_eq!(
        events,
        world.logged(&second),
        "the run asked for, read whole"
    );

    world.slept(&first);
    let came = world.slept(&second);
    let RunNews::Events(batch) = next_of(&mut following, &mut kept).await else {
        panic!("the run followed now is read on");
    };
    assert_eq!(seqs(&batch), vec![came]);
}

/// The waits a `next` is dropped after, from at once to past a look: each drop lands
/// somewhere else in the following's work, between two looks or inside one.
const DROPPED_AFTER_MS: [u64; 8] = [0, 1, 2, 5, 20, 60, 110, 150];

/// A `next` dropped wherever it stands loses nothing a look read: what came is handed
/// later, whole, once and in the log's order.
#[tokio::test]
async fn dropping_next_between_looks_loses_no_news() {
    let world = World::new();
    let (run, dir) = world.born("dropped").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;
    let stop = Stop::new();

    /// `news` folded into `kept`, the seqs of the events it hands kept in `came`.
    fn take(news: FollowNews, kept: &mut FollowedRun, came: &mut Vec<Seq>) {
        let FollowNews::Run { run, news } = news else {
            panic!("only news of the run held comes, and came {news:?}");
        };
        assert_eq!(run, kept.run);
        if let RunNews::Events(events) = &news {
            came.extend(seqs(events));
        }
        kept.took(&news);
    }
    let (mut recorded, mut came) = (Vec::new(), Vec::new());
    for after in DROPPED_AFTER_MS {
        recorded.push(world.slept(&run));
        recorded.push(world.slept(&run));
        let dropped =
            tokio::time::timeout(Duration::from_millis(after), following.next(&stop)).await;
        if let Ok(news) = dropped {
            take(news.unwrap(), &mut kept, &mut came);
        }
    }
    while came.len() < recorded.len() {
        take(next(&mut following).await, &mut kept, &mut came);
    }
    assert_eq!(
        came, recorded,
        "every event handed once, in the log's order"
    );
    assert_eq!(kept.state, RunState::replay(&world.logged(&run)));
}

#[tokio::test]
async fn a_tripped_stop_ends_following_with_the_stop() {
    let world = World::new();
    let (run, dir) = world.born("moving").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    whole(&mut following).await;
    let stop = Stop::new();
    stop.stop();
    assert!(matches!(following.next(&stop).await, Err(Error::Stopped)));
}

#[tokio::test]
async fn a_run_that_closes_promoted_is_followed_onto_its_successor() {
    let world = World::new();
    let (run, dir) = world.born("promoted").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    world.record(&run, closed_promoted());
    let successor = world.successor_of(&run, "successor").await;

    assert!(matches!(
        next_of(&mut following, &mut kept).await,
        RunNews::Events(_)
    ));
    let RunNews::Successor {
        run: next_run,
        events,
    } = next_of(&mut following, &mut kept).await
    else {
        panic!("the successor is followed on");
    };
    assert_eq!(next_run.run, successor);
    assert_eq!(
        events,
        world.logged(&successor),
        "the successor's log as it stands"
    );
    assert_eq!(kept.run, successor, "the run held is the successor");
    assert!(!following.awaits_successor());
}

/// A copy of the run followed, taken from the run read whole and handed each piece of
/// news, stands where the following does: onto the successor, at its head.
#[tokio::test]
async fn a_followed_run_handed_each_news_stands_where_the_following_does() {
    let world = World::new();
    let (run, dir) = world.born("copied").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    world.record(&run, closed_promoted());
    let successor = world.successor_of(&run, "copied-on").await;
    while kept.run != successor {
        next_of(&mut following, &mut kept).await;
    }

    assert_eq!(kept.state, RunState::replay(&world.logged(&successor)));
    let registry = yunta_engine::read_engine(&dir.with_file_name(successor.as_str()), &SystemProbe);
    assert_eq!(kept.engine, registry);
}

/// A run parked by the engine that drove it, then resumed by another: the pause is
/// handed, and the following reads on past the resume.
#[tokio::test]
async fn a_run_parked_then_resumed_from_another_process_is_read_on() {
    let world = World::new();
    let (run, dir) = world.born("parked").await;
    let engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let paused = world.record(
        &run,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload {
            pause: PauseReason::Cancelled,
        })),
    );
    drop(engine);
    assert_eq!(
        read_on(&mut following, &mut kept, &[paused]).await,
        vec![paused]
    );
    assert!(!world.frame(&kept).moves(), "the run is parked");

    let _resumed_by = driven_by_this_process(&dir);
    let resumed = world.record(
        &run,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None))),
    );
    let slept = world.slept(&run);
    assert_eq!(
        read_on(&mut following, &mut kept, &[resumed, slept]).await,
        vec![resumed, slept]
    );
    assert!(world.frame(&kept).moves(), "the run moves again");
}

/// The seqs of the events `following` hands until every one of `awaited` came, past
/// what the registry says of the engine meanwhile, each folded into `kept`.
async fn read_on(following: &mut Following, kept: &mut FollowedRun, awaited: &[Seq]) -> Vec<Seq> {
    let mut came = Vec::new();
    while !awaited.iter().all(|seq| came.contains(seq)) {
        match next_of(following, kept).await {
            RunNews::Events(events) => came.extend(seqs(&events)),
            RunNews::Engine(_) => {}
            other => panic!("the following goes on reading the run, and handed {other:?}"),
        }
    }
    came
}

/// How long a following owes a gone engine before it tells of it, with nothing
/// committed: the two seconds a stall is owed.
const STALL_OWED: Duration = Duration::from_secs(2);

/// How long a test waits on a following that has nothing to hand: longer than the
/// second between two reads of the registry, so a read that hands something again
/// shows.
const NOTHING_COMES: Duration = Duration::from_millis(1_500);

/// An engine killed under a following commits nothing: the registry tells of it, the
/// run reads stalled, and the following goes on, since where a run stands is a reading
/// of its frame.
#[tokio::test]
async fn an_engine_killed_shows_as_a_stall_and_the_following_goes_on() {
    let world = World::new();
    let (run, dir) = world.born("killed").await;
    let _engine = driven_by_this_process(&dir);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;
    let stop = Stop::new();

    left_by_a_dead_engine(&dir);
    let news = tokio::time::timeout(STALL_OWED, following.next(&stop))
        .await
        .expect("the gone engine is told within two seconds")
        .expect("following reads the run");
    let FollowNews::Run { news, .. } = news else {
        panic!("news of the run held, and came {news:?}");
    };
    assert!(matches!(news, RunNews::Engine(Engine::Dead)), "{news:?}");
    kept.took(&news);
    assert_eq!(world.frame(&kept).standing, Standing::Stalled);
    let after = tokio::time::timeout(NOTHING_COMES, following.next(&stop)).await;
    assert!(
        after.is_err(),
        "the following goes on, waiting for what comes, and handed {after:?}"
    );
}

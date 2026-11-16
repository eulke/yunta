//! What a look at a followed run and field costs, counted on the reader the following
//! keeps, and what waits on the following's own clocks, which no caller outside the
//! module can move: a wait for a successor run out, a run's directory checked. Over a log
//! a bench writes.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::time::Instant;
use yunta_core::events::{
    EventDraft, EventPayload, HostSuspendedPayload, RunEvent, RunFinishedPayload, RunMetrics,
    TerminalState, TokenUsage,
};
use yunta_core::persisted::PersistedDoc;
use yunta_core::{ConfigLayer, Env, ProcessSecrets, RunId, SystemIdSource};
use yunta_render::page::RunUnread;
use yunta_testkit::{Bench, SourceLog};
use yunta_testkit_core::FixedClock;

use super::field::DIRS_EVERY;
use super::following::{SuccessorWait, SUCCESSOR_WAIT};
use super::{FollowNews, Followed, Following, RunNews};
use crate::{FieldReach, Project, Stop, Workspace};

/// One node, so a run of it has a frame to stand in.
const WORKFLOW: &str = "name: followed\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

/// How many looks the test takes at a store where nothing moves: several, so a look
/// that costs more than the first shows.
const LOOKS: u64 = 5;

/// The project whose runs and log are `bench`'s, under a clock that stands still.
fn project_of(bench: &Bench) -> Project {
    Project {
        config: ConfigLayer::default(),
        runs_root: bench.runs_root.clone(),
        worktrees_root: bench.runs_root.with_file_name("worktrees"),
        storage_path: bench.store_path().to_path_buf(),
        user_root: bench.runs_root.with_file_name("user"),
        workspace: Workspace {
            dir: bench.worktree.clone(),
            env: Env::default(),
            clock: Arc::new(FixedClock),
            ids: Arc::new(SystemIdSource),
            secrets: Arc::new(ProcessSecrets),
            binary: PathBuf::from("yunta"),
        },
    }
}

/// Run `run`, born on `bench`'s log with the manifest it froze in its directory, and
/// nothing since. No registry stands beside it, which reads as no engine yet, never as
/// a stall, so the run moves and a following of it looks on.
async fn born(bench: &Bench, run: &str) -> RunId {
    let run = RunId::from(run);
    let dir = bench.runs_root.join(run.as_str());
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = bench.manifest_for(WORKFLOW, "").await;
    let bytes = PersistedDoc::of(manifest).write().unwrap();
    std::fs::write(yunta_engine::run_dir::manifest_path(&dir), bytes).unwrap();
    SourceLog::open(&bench.storage, &run, Arc::new(FixedClock));
    run
}

#[tokio::test]
async fn a_look_at_a_store_where_nothing_moved_costs_one_statement() {
    let bench = Bench::with_run_id("followed");
    let project = project_of(&bench);
    let run = born(&bench, "quiet").await;
    let mut following = followed(&project, &run).await;

    let before = following.reader.statements();
    for look in 1..=LOOKS {
        following.look(&Stop::new()).await.unwrap();
        let cost = following.reader.statements() - before;
        assert_eq!(
            cost, look,
            "{look} looks where nothing moved ran {cost} statements"
        );
    }
    assert!(
        following.news.is_empty(),
        "nothing moved, so nothing is handed"
    );
}

/// How many runs the field of a test holds: several, so a look whose cost grows with
/// the field shows.
const RUNS: usize = 3;

/// A field of three runs followed beside one of them read whole costs one statement a
/// look while nothing moves: the runs held add no read of the log.
#[tokio::test]
async fn a_look_at_a_field_where_nothing_moved_costs_one_statement() {
    let bench = Bench::with_run_id("followed");
    let project = project_of(&bench);
    let mut runs = Vec::new();
    for run in 0..RUNS {
        runs.push(born(&bench, &format!("quiet-{run}")).await);
    }
    let followed = Followed {
        field: Some(FieldReach::Machine),
        run: Some(runs[0].clone()),
    };
    let mut following = project.follow(followed).await.unwrap();
    let first: Vec<FollowNews> = following.news.drain(..).collect();
    assert!(
        matches!(first.as_slice(), [FollowNews::Field(field), FollowNews::Whole { .. }] if field.runs.len() == RUNS),
        "the field read whole, then the run: {first:?}"
    );

    let before = following.reader.statements();
    for look in 1..=LOOKS {
        following.look(&Stop::new()).await.unwrap();
        let cost = following.reader.statements() - before;
        assert_eq!(
            cost, look,
            "{look} looks where nothing moved ran {cost} statements"
        );
    }
    assert!(
        following.news.is_empty(),
        "nothing moved, so nothing is handed"
    );
}

/// Collection takes a run in two passes, its directory, then its rows: followed in
/// the field, the run is said unreadable for its directory once it is checked, then
/// leaves with its rows; the run beside it is read on.
#[tokio::test]
async fn a_collected_run_passes_through_its_missing_directory_and_leaves() {
    let bench = Bench::with_run_id("followed");
    let project = project_of(&bench);
    let collected = born(&bench, "collected").await;
    let kept = born(&bench, "kept").await;
    let followed = Followed {
        field: Some(FieldReach::Machine),
        run: None,
    };
    let mut following = project.follow(followed).await.unwrap();
    following.news.clear();

    std::fs::remove_dir_all(bench.runs_root.join(collected.as_str())).unwrap();
    let field = following.field.as_mut().expect("the field is held");
    field.dirs_checked_at = Instant::now() - DIRS_EVERY;
    following.look(&Stop::new()).await.unwrap();
    let handed: Vec<FollowNews> = following.news.drain(..).collect();
    let [FollowNews::Unreadable(unread)] = handed.as_slice() else {
        panic!("the run without its directory is said unreadable: {handed:?}");
    };
    assert_eq!(unread.run_id, collected);
    let runs_root = bench.runs_root.clone();
    assert_eq!(unread.problem, RunUnread::NoDirectory { runs_root });

    bench.storage.purge_run(&collected).unwrap();
    let draft = EventDraft::new(
        kept.clone(),
        None,
        EventPayload::Run(RunEvent::HostSuspended(HostSuspendedPayload::slept(
            std::time::Duration::from_secs(3),
        ))),
    )
    .unwrap();
    let seq = bench.storage.append(&draft, &FixedClock).unwrap();
    following.look(&Stop::new()).await.unwrap();
    let handed: Vec<FollowNews> = following.news.drain(..).collect();
    let left = handed
        .iter()
        .any(|news| matches!(news, FollowNews::Left(run) if *run == collected));
    assert!(left, "the run leaves with its rows: {handed:?}");
    let read_on = handed.iter().any(|news| {
        matches!(news, FollowNews::Run { run, news: RunNews::Events(events) }
            if *run == kept && events.iter().map(|event| event.seq).eq([seq]))
    });
    assert!(read_on, "the run beside it is read on: {handed:?}");
}

/// `run` followed in `project`, the run read whole already handed: what is left in the
/// following's queue is what its looks found.
async fn followed(project: &Project, run: &RunId) -> Following {
    let followed = Followed {
        field: None,
        run: Some(run.clone()),
    };
    let mut following = project.follow(followed).await.unwrap();
    let first = following.news.pop_front();
    assert!(
        matches!(first, Some(FollowNews::Whole { .. })),
        "the run read whole comes first: {first:?}"
    );
    following
}

/// A run closed promoted whose successor never comes is handed once as having none,
/// once it waited the time one takes to be created; the following awaits a successor
/// until that news is handed, and no longer after.
#[tokio::test]
async fn a_wait_for_a_successor_that_runs_out_is_handed_once() {
    let bench = Bench::with_run_id("followed");
    let project = project_of(&bench);
    let run = born(&bench, "promoted").await;
    let closed = RunFinishedPayload::closed(
        TerminalState::Promoted,
        RunMetrics::of(TokenUsage::default(), 0),
    );
    let draft = EventDraft::new(
        run.clone(),
        None,
        EventPayload::Run(RunEvent::Finished(closed)),
    )
    .unwrap();
    bench.storage.append(&draft, &FixedClock).unwrap();
    let mut following = followed(&project, &run).await;
    assert!(following.awaits_successor(), "a successor is awaited");

    let held = following.held.as_mut().expect("the run is held");
    held.successor = SuccessorWait::Awaited {
        since: Instant::now() - SUCCESSOR_WAIT,
    };
    for _ in 0..LOOKS {
        following.look(&Stop::new()).await.unwrap();
    }
    assert!(
        following.awaits_successor(),
        "awaited until the news is handed"
    );
    let handed: Vec<FollowNews> = following.news.drain(..).collect();
    let none_came = handed
        .iter()
        .filter(|news| {
            matches!(
                news,
                FollowNews::Run {
                    news: RunNews::NoSuccessor,
                    ..
                }
            )
        })
        .count();
    assert_eq!(none_came, 1, "{handed:?}");
    assert!(!following.awaits_successor(), "no longer awaited");
}

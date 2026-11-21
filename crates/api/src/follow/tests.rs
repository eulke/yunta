//! What a look at a followed run and field costs, counted on the reader the following
//! keeps, and what waits on the following's own clocks, which no caller outside the
//! module can move: a wait for a successor run out, and final once handed; a run's
//! directory checked; and the order a reading of what a run asks takes among the events,
//! its read and the following's next look made to wait on each other. Over a log a bench
//! writes.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::time::Instant;
use yunta_core::events::{
    EventDraft, EventPayload, GateEvent, HostSuspendedPayload, RunCreatedPayload, RunEvent,
    RunFinishedPayload, RunMetrics, TerminalState, TokenUsage,
};
use yunta_core::persisted::PersistedDoc;
use yunta_core::{ConfigLayer, Env, ProcessSecrets, RunId, SystemIdSource};
use yunta_render::page::RunUnread;
use yunta_testkit::workflows::{ASKS, HANDS_ONE_QUESTION_OVER};
use yunta_testkit::{Bench, SourceLog};
use yunta_testkit_core::FixedClock;

use super::field::DIRS_EVERY;
use super::successor::SuccessorWait;
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
            forges: Arc::new(crate::DeclaredForge),
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

/// What `following` handed once the reading of what its run asks, under way, came back:
/// the reading waited on, and handed as the following hands it.
async fn read_of_what_it_asks(following: &mut Following) -> Vec<FollowNews> {
    let read = following.asking.read.recv().await;
    let (turn, news) = read.expect("a reading of what the run asks is under way");
    following.asked_read(turn, news);
    following.news.drain(..).collect()
}

/// What a run asks is read as of the head the following holds, never past it: a round read
/// again once its answer reached the log, before the following read that answer, is handed
/// as the pause the run stands at by that head; then come the events that answer it, then
/// that nothing waits. Not that nothing waits first, ahead of the events that say why,
/// which a screen holding the round takes for a pause gone with nothing answered.
#[tokio::test]
async fn a_round_read_again_as_its_answer_lands_is_read_at_the_head_held() {
    let bench = Bench::with_run_id("01M7R0VNDREADAGA1N00000000");
    bench.run(ASKS, HANDS_ONE_QUESTION_OVER).await;
    let project = project_of(&bench);
    let mut following = followed(&project, &bench.run_id).await;
    let first = read_of_what_it_asks(&mut following).await;
    let [FollowNews::Run {
        news: RunNews::Awaited(Some((parked_at, _))),
        ..
    }] = first.as_slice()
    else {
        panic!("the round the run is parked on is read: {first:?}");
    };

    // The answer reaches the log; then the round is read again, as a following made anew
    // or one that holds the run again reads it, the run still held before the answer.
    let recorded = bench.answer_round("ask", &[("summary", "the greeting")]);
    recorded.await.expect("the answer is recorded");
    following.asking.forget();
    following.ask(project.workspace.clock.now());
    let again = read_of_what_it_asks(&mut following).await;
    let [FollowNews::Run {
        news: RunNews::Awaited(Some((read_at, _))),
        ..
    }] = again.as_slice()
    else {
        panic!("the round is read as the head held stands: {again:?}");
    };
    assert_eq!(read_at, parked_at);

    following.look(&Stop::new()).await.unwrap();
    let handed: Vec<FollowNews> = following.news.drain(..).collect();
    let [FollowNews::Run {
        news: RunNews::Events(events),
        ..
    }, FollowNews::Run {
        news: RunNews::Awaited(None),
        ..
    }] = handed.as_slice()
    else {
        panic!("the events that answer it, then that nothing waits: {handed:?}");
    };
    let answered = events.iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Gates(GateEvent::QuestionsAnswered(_)))
        )
    });
    assert!(answered, "{events:?}");
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
        until: Instant::now(),
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

/// Appends `payload` to `run`'s log on `bench`'s store.
fn appended(bench: &Bench, run: &RunId, payload: EventPayload) {
    let draft = EventDraft::new(run.clone(), None, payload).unwrap();
    bench.storage.append(&draft, &FixedClock).unwrap();
}

/// Run `run`, born on `bench`'s log as promoted from `from`, its manifest beside it.
async fn born_promoted_from(bench: &Bench, from: &RunId, run: &str) -> RunId {
    let dir = bench.runs_root.join(run);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = bench.manifest_for(WORKFLOW, "").await;
    let bytes = PersistedDoc::of(manifest).write().unwrap();
    std::fs::write(yunta_engine::run_dir::manifest_path(&dir), bytes).unwrap();
    let run = RunId::from(run);
    let created = RunCreatedPayload {
        checkout: None,
        manifest_hash: yunta_core::sha256_hex(run.as_str().as_bytes()),
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
    appended(bench, &run, EventPayload::Run(RunEvent::Created(created)));
    run
}

/// How many statements a look past a commit to one run of the field costs, the run held
/// standing closed promoted with none come: whether the log moved, the runs' heads, and
/// what came past that run's head. Nothing asks after a successor.
const A_LOOK_PAST_A_COMMIT: u64 = 3;

/// Whether `handed` holds news of a run that `is` takes.
fn holds(handed: &[FollowNews], is: fn(&RunNews) -> bool) -> bool {
    handed
        .iter()
        .any(|news| matches!(news, FollowNews::Run { news, .. } if is(news)))
}

/// That none came is final: once it is handed, a look past a later commit asks after no
/// successor, and a run the log then holds that names the promoted run joins the field as
/// any run new to the log does, no successor handed.
#[tokio::test]
async fn none_come_is_final() {
    let bench = Bench::with_run_id("followed");
    let project = project_of(&bench);
    let beside = born(&bench, "beside").await;
    let run = born(&bench, "promoted").await;
    let tokens = RunMetrics::of(TokenUsage::default(), 0);
    let closed = RunFinishedPayload::closed(TerminalState::Promoted, tokens);
    appended(&bench, &run, EventPayload::Run(RunEvent::Finished(closed)));
    let followed = Followed {
        field: Some(FieldReach::Machine),
        run: Some(run.clone()),
    };
    let mut following = project.follow(followed).await.unwrap();
    let held = following.held.as_mut().expect("the run is held");
    held.successor = SuccessorWait::Awaited {
        until: Instant::now(),
    };
    following.look(&Stop::new()).await.unwrap();
    let handed: Vec<FollowNews> = following.news.drain(..).collect();
    assert!(holds(&handed, |news| matches!(news, RunNews::NoSuccessor)));

    let slept = HostSuspendedPayload::slept(std::time::Duration::from_secs(3));
    appended(
        &bench,
        &beside,
        EventPayload::Run(RunEvent::HostSuspended(slept)),
    );
    let before = following.reader.statements();
    following.look(&Stop::new()).await.unwrap();
    let cost = following.reader.statements() - before;
    assert_eq!(cost, A_LOOK_PAST_A_COMMIT, "a look asks after no successor");

    let late = born_promoted_from(&bench, &run, "late").await;
    following.look(&Stop::new()).await.unwrap();
    let handed: Vec<FollowNews> = following.news.drain(..).collect();
    let joined = |news: &FollowNews| matches!(news, FollowNews::Joined(it) if it.run == late);
    assert!(handed.iter().any(joined), "it joins the field: {handed:?}");
    let handed_on = holds(&handed, |news| matches!(news, RunNews::Successor { .. }));
    assert!(!handed_on, "no successor after none came: {handed:?}");
}

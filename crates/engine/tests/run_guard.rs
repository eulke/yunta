//! A run has one engine: `hold_run` is the only way to claim a run, exactly one
//! attempt gets it, and a holder that is gone never blocks the next one.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use tokio::sync::Barrier;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventPayload, GateEvent, GateResolvedPayload, HumanChoice, PauseReason, RunEvent,
    RunPausedPayload, RunResumedPayload,
};
use yunta_core::persisted::{Persisted, PersistedDoc};
use yunta_core::process::signal::Liveness;
use yunta_core::Pid;
use yunta_engine::lock::{Contention, LockError, LockOwner};
use yunta_engine::run_dir::engine_lock;
use yunta_engine::{contention_for, hold_refused, hold_run, RunHold};
use yunta_replay::RunState;
use yunta_testkit_core::{FixedClock, Log};

async fn hold(run_dir: &Path, contention: Contention) -> Result<RunHold, LockError> {
    hold_run(run_dir, contention, &FixedClock, &CancellationToken::new()).await
}

/// Writes the lock a process that is not this test left behind.
fn names_holder(run_dir: &Path, pid: Pid, started_at: DateTime<Utc>) {
    let path = engine_lock(run_dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let record = LockOwner {
        schema_version: LockOwner::SCHEMA_VERSION,
        pid,
        started_at,
    };
    std::fs::write(&path, PersistedDoc::of(record).write().unwrap()).unwrap();
}

/// The pid of a process that has already ended and been collected.
fn a_pid_nothing_runs_under() -> Pid {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    Pid::try_from(child.id()).unwrap()
}

/// Later than any process on this host started, so nothing reads as having taken
/// the holder's pid after the lock was written.
fn after_any_process_started() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap()
}

/// What replay derives from a log whose last event is a pause.
fn parked() -> RunState {
    RunState::replay(&parking().build())
}

/// The log of a run that just parked.
fn parking() -> Log {
    Log::for_run("run-1").event(EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
        &PauseReason::Recorded {
            sentence: "parked".into(),
        },
    ))))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn of_many_attempts_at_once_exactly_one_holds_the_run() {
    /// More attempts than worker threads, so they contend on any machine.
    const ATTEMPTS: usize = 8;
    let dir = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(ATTEMPTS));
    let attempts: Vec<_> = (0..ATTEMPTS)
        .map(|_| {
            let barrier = barrier.clone();
            let run_dir: PathBuf = dir.path().to_path_buf();
            tokio::spawn(async move {
                barrier.wait().await;
                hold(&run_dir, Contention::Refuse).await
            })
        })
        .collect();
    let mut answers = Vec::new();
    for attempt in attempts {
        answers.push(attempt.await.unwrap());
    }
    // Every answer is kept until here, so the winner still holds while the
    // others are counted: with no lock, each of them would be `Ok`.
    let winners = answers.iter().filter(|answer| answer.is_ok()).count();
    assert_eq!(winners, 1, "got: {answers:?}");
    for answer in &answers {
        assert!(
            matches!(answer, Ok(_) | Err(LockError::Held { .. })),
            "got: {answer:?}"
        );
    }
}

#[tokio::test]
async fn a_holder_that_died_is_replaced_and_its_lock_goes_with_the_new_hold() {
    let dir = tempfile::tempdir().unwrap();
    names_holder(
        dir.path(),
        a_pid_nothing_runs_under(),
        after_any_process_started(),
    );
    let held = hold(dir.path(), Contention::Refuse)
        .await
        .expect("a dead holder never blocks a run");
    drop(held);
    assert!(
        !engine_lock(dir.path()).exists(),
        "the new holder removes the lock when it lets go"
    );
}

/// The real process table, on both platforms: this process started after the
/// time the record names, so the record's holder is somebody else who had the
/// same pid.
#[tokio::test]
async fn a_pid_reused_by_a_later_process_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let long_ago = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    names_holder(dir.path(), Pid::current(), long_ago);
    let _held = hold(dir.path(), Contention::Refuse)
        .await
        .expect("a process that started after the lock is not its holder");
}

/// Pid 1 runs as root, so this process cannot signal it, and it started before
/// the time the record names: nothing shows that it is not the holder.
#[tokio::test]
async fn a_holder_that_cannot_be_ruled_out_is_never_taken() {
    let dir = tempfile::tempdir().unwrap();
    names_holder(
        dir.path(),
        Pid::try_from(1_i32).unwrap(),
        after_any_process_started(),
    );
    let answer = hold(dir.path(), Contention::Refuse).await;
    assert!(
        matches!(answer, Err(LockError::Held { .. })),
        "got: {answer:?}"
    );
    assert!(engine_lock(dir.path()).exists());
}

#[tokio::test]
async fn a_lock_nobody_can_read_is_never_taken() {
    let dir = tempfile::tempdir().unwrap();
    let path = engine_lock(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not a holder").unwrap();
    let answer = hold(dir.path(), Contention::Refuse).await;
    assert!(
        matches!(answer, Err(LockError::Unreadable { .. })),
        "got: {answer:?}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a holder");
}

#[tokio::test]
async fn a_run_someone_drives_is_refused_at_once_naming_the_holder() {
    let dir = tempfile::tempdir().unwrap();
    let first = hold(dir.path(), Contention::Refuse).await.unwrap();
    match hold(dir.path(), Contention::Refuse).await {
        Err(LockError::Held {
            owner,
            liveness: Liveness::Alive,
            ..
        }) => assert_eq!(owner.pid, Pid::current()),
        other => panic!("got: {other:?}"),
    }
    assert!(
        engine_lock(dir.path()).exists(),
        "a refused attempt leaves the holder's lock alone"
    );
    drop(first);
}

/// The next engine is still waiting when the time runs out here: one that gave up
/// or took the lock would have answered. Only then does the first let go.
#[tokio::test]
async fn a_parked_runs_next_driver_waits_for_the_engine_that_parked_it() {
    let dir = tempfile::tempdir().unwrap();
    let leaving = hold(dir.path(), Contention::Refuse).await.unwrap();
    let next = hold(dir.path(), contention_for(&parked()));
    tokio::pin!(next);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut next)
            .await
            .is_err(),
        "the next engine waits while the engine that parked the run leaves"
    );
    drop(leaving);
    let held = tokio::time::timeout(Duration::from_secs(10), next)
        .await
        .expect("the wait ends when the engine lets go")
        .expect("the run is free once the engine that parked it has left");
    assert_eq!(held.run_dir(), dir.path());
}

#[tokio::test]
async fn a_wait_that_runs_out_names_the_holder() {
    let dir = tempfile::tempdir().unwrap();
    let first = hold(dir.path(), Contention::Refuse).await.unwrap();
    let brief = Contention::Wait {
        patience: Duration::from_millis(50),
        poll: Duration::from_millis(5),
    };
    match hold(dir.path(), brief).await {
        Err(LockError::Timeout {
            owner: Some(owner), ..
        }) => assert_eq!(owner.pid, Pid::current()),
        other => panic!("got: {other:?}"),
    }
    drop(first);
}

#[tokio::test]
async fn an_interrupt_ends_the_wait() {
    let dir = tempfile::tempdir().unwrap();
    let first = hold(dir.path(), Contention::Refuse).await.unwrap();
    let interrupted = CancellationToken::new();
    interrupted.cancel();
    let patient = Contention::Wait {
        patience: Duration::from_secs(60),
        poll: Duration::from_millis(5),
    };
    let answer = hold_run(dir.path(), patient, &FixedClock, &interrupted).await;
    assert!(
        matches!(answer, Err(LockError::Cancelled { .. })),
        "got: {answer:?}"
    );
    drop(first);
}

#[tokio::test]
async fn letting_go_frees_the_run_for_this_same_process() {
    let dir = tempfile::tempdir().unwrap();
    let first = hold(dir.path(), Contention::Refuse).await.unwrap();
    assert!(engine_lock(dir.path()).exists());
    drop(first);
    assert!(!engine_lock(dir.path()).exists());
    let _held = hold(dir.path(), Contention::Refuse)
        .await
        .expect("a released run can be held again");
}

/// A run directory with no scratch can be resumed on the base, since a node's
/// staging creates it: the guard must not be what breaks that.
#[tokio::test]
async fn a_run_directory_without_its_scratch_can_be_held() {
    let dir = tempfile::tempdir().unwrap();
    let held = hold(dir.path(), Contention::Refuse)
        .await
        .expect("the scratch is created for the lock");
    assert_eq!(held.run_dir(), dir.path());
}

#[tokio::test]
async fn what_a_hold_would_be_refused_with_is_read_without_taking_anything() {
    let dir = tempfile::tempdir().unwrap();
    assert!(hold_refused(dir.path()).await.is_none(), "a free run");

    let first = hold(dir.path(), Contention::Refuse).await.unwrap();
    assert!(
        matches!(
            hold_refused(dir.path()).await,
            Some(LockError::Held {
                liveness: Liveness::Alive,
                ..
            })
        ),
        "a run someone drives"
    );
    drop(first);

    names_holder(
        dir.path(),
        a_pid_nothing_runs_under(),
        after_any_process_started(),
    );
    assert!(hold_refused(dir.path()).await.is_none(), "a dead holder");
    assert!(
        engine_lock(dir.path()).exists(),
        "reading replaces nothing: the next holder does"
    );
}

#[test]
fn a_parked_run_is_waited_for_and_any_other_is_refused_at_once() {
    assert!(matches!(contention_for(&parked()), Contention::Wait { .. }));
    assert!(matches!(
        contention_for(&RunState::replay(&[])),
        Contention::Refuse
    ));
}

/// An answer can reach the log while the engine that parked the run is still leaving,
/// and the resume it starts meets that engine: the run is still parked.
#[test]
fn a_parked_run_with_an_answer_on_its_log_is_waited_for() {
    let answered = RunState::replay(
        &parking()
            .event(EventPayload::Gates(GateEvent::Resolved(
                GateResolvedPayload::Chosen(HumanChoice {
                    option: "approve".into(),
                    by: "mcp".into(),
                    free_text: None,
                }),
            )))
            .build(),
    );
    assert!(matches!(contention_for(&answered), Contention::Wait { .. }));
}

/// What a resume wrote ends the pause, so a run that woke is moving again.
#[test]
fn a_run_that_woke_after_its_pause_is_refused_at_once() {
    let woken = RunState::replay(
        &parking()
            .event(EventPayload::Run(RunEvent::Resumed(
                RunResumedPayload::new(Vec::new(), None),
            )))
            .build(),
    );
    assert!(matches!(contention_for(&woken), Contention::Refuse));
}

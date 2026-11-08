//! What a frame measures at the instant it is read at: how long a person
//! has been asked at a terminal, how long the run has stood where it is, and
//! where its time went. A surface says these durations; none of them
//! subtracts two instants of its own.

use super::*;
use yunta_core::Pid;
use yunta_replay::registry::Prompt;

/// The pid of the process the registry names.
fn engine_pid() -> Pid {
    Pid::try_from(4242).expect("a pid")
}

/// What a run's registry says while its engine asks a person about `node`
/// at its terminal, since `since_secs`.
fn asking(node: &str, since_secs: i64) -> Engine {
    Engine::Alive {
        pid: engine_pid(),
        prompt: Some(Prompt {
            node: Some(node.into()),
            since: at(since_secs),
        }),
        words: None,
    }
}

/// A run whose `build` has been working since `last_secs`, its last event.
fn building(last_secs: i64) -> Vec<StoredEvent> {
    log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (last_secs, Some("build"), started(1)),
    ])
}

fn read(events: &[StoredEvent], engine: &Engine, now_secs: i64) -> RunFrame {
    RunFrame::of(
        &RUN.into(),
        &chain(),
        &RunState::replay(events),
        None,
        engine,
        at(now_secs),
    )
}

#[test]
fn a_frame_measures_how_long_its_engine_has_asked_and_its_run_has_stood() {
    let frame = read(&building(70), &asking("build", 0), 190);

    let asking = frame.asking.expect("the engine asks");
    assert_eq!(asking.pid, engine_pid());
    assert_eq!(asking.prompt.node, Some("build".into()));
    assert_eq!(asking.waited, Duration::from_secs(3 * 60 + 10));
    assert_eq!(frame.since_last_event, Some(Duration::from_secs(2 * 60)));
}

#[test]
fn a_frame_read_before_its_last_event_has_stood_there_no_time() {
    let frame = read(&building(70), &this_process(), 60);

    assert_eq!(frame.since_last_event, Some(Duration::ZERO));
}

#[test]
fn a_prompt_stamped_after_the_frame_has_waited_no_time() {
    let frame = read(&building(70), &asking("build", 200), 190);

    assert_eq!(
        frame.asking.map(|asking| asking.waited),
        Some(Duration::ZERO)
    );
}

#[test]
fn a_frame_whose_engine_asks_no_one_holds_no_asking() {
    let frame = read(&building(70), &this_process(), 190);

    assert_eq!(frame.asking, None);
}

#[test]
fn a_frame_of_an_empty_log_has_stood_nowhere() {
    let frame = read(&[], &this_process(), 190);

    assert_eq!(frame.since_last_event, None);
}

#[test]
fn an_open_run_counts_its_time_to_the_frames_instant() {
    let events = building(3);
    let earlier = read(&events, &this_process(), 10);
    let later = read(&events, &this_process(), 70);

    assert_eq!(
        later.time.working - earlier.time.working,
        Duration::from_secs(60),
        "a node at work counts its time up to the instant the run is read at"
    );
}

#[test]
fn an_ended_run_counts_its_time_to_its_last_event() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (3, Some("build"), started(1)),
        (9, Some("build"), finished()),
        (10, Some("fix"), started(1)),
        (11, Some("fix"), finished()),
        (12, None, run_finished(TerminalState::Done)),
    ]);
    let soon = read(&events, &this_process(), 100);
    let long_after = read(&events, &this_process(), 1000);

    assert!(soon.time.working > Duration::ZERO, "{:?}", soon.time);
    assert_eq!(
        soon.time, long_after.time,
        "a run that ended spends no more time"
    );
}

//! Where a run's wall-clock went, read off its log: what was open across
//! each stretch between two events, or what closed it when nothing was.
//!
//! Built from events in their wire shape, the one the log persists.

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{EventBody, StoredEvent};
use yunta_engine::{compute_run_stats, TimeSpent};

/// A log of `(seconds since the run began, node, payload as the log
/// persists it)`.
fn log(entries: &[(i64, Option<&str>, &str)]) -> Vec<StoredEvent> {
    let in_ms: Vec<(i64, Option<&str>, &str)> = entries
        .iter()
        .map(|(at, node, payload)| (at * 1000, *node, *payload))
        .collect();
    log_ms(&in_ms)
}

/// [`log`], its instants in milliseconds since the run began.
fn log_ms(entries: &[(i64, Option<&str>, &str)]) -> Vec<StoredEvent> {
    let start: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    entries
        .iter()
        .enumerate()
        .map(|(index, (at, node, payload))| {
            let serde_json::Value::Object(object) = serde_json::from_str(payload).unwrap() else {
                panic!("a payload is an object: {payload}");
            };
            StoredEvent {
                run_id: "run-time".into(),
                seq: (index as u64 + 1).into(),
                timestamp: start + chrono::Duration::milliseconds(*at),
                node_id: node.map(Into::into),
                body: EventBody::from_object(object, 1).unwrap(),
            }
        })
        .collect()
}

fn time_of(events: &[StoredEvent]) -> TimeSpent {
    let workflow: yunta_core::Workflow =
        yunta_core::yaml::parse("name: timed\nnodes:\n  - { id: a, kind: bash, run: \"true\" }\n")
            .unwrap();
    compute_run_stats(&workflow, events).time
}

const CREATED: &str = r#"{"kind":"run_created","manifest_hash":"03a119e94d983cbf5279c0959eca2fa361b3db64414f223eb0ec83f2cad41ae9","inputs":{},"mode":"quick","base_branch":"main","base_commit":"d07a58e99f630cc0567076fa7d4cd19c7c6a3b49"}"#;
const MEASURED: &str = r#"{"kind":"baseline_captured","command":"cargo test","results":{"exit_code":0,"summary":"ok"},"hash":"040fb4c74cc0f56e630cd65d8a3a296f6abefa441bbb81df7b7e65cf16209af3","origin":{"type":"measured"}}"#;
const STARTED: &str = r#"{"kind":"node_started","attempt":1}"#;
const FINISHED: &str =
    r#"{"kind":"node_finished","outcome":"ok","tokens_used":{"input":0,"output":0}}"#;
const ASKED: &str = r#"{"kind":"questions_asked","questions_hash":"7f6a1f4281c82bad69e3d2ecac0e87f6ebfb21f7f07f73e248d268210cc010ee","questions":["scope"],"tokens_used":{"input":0,"output":0}}"#;
const ANSWERED: &str = r#"{"kind":"questions_answered","answers_hash":"e7754c509612229b458829f79b454a00aee04a5f4848001fa6fef0ff74a2d8a1","channel":"tty"}"#;
const WAITING: &str = r#"{"kind":"gate_waiting","summary":"Approve?","evidence":[],"options":[{"id":"approve","label":"approve","tradeoff":"resolves this gate"}]}"#;
const RESOLVED: &str =
    r#"{"kind":"gate_resolved","chosen_option":"approve","resolved_by":"unverified:someone"}"#;
const PAUSED: &str = r#"{"kind":"run_paused","reason":"cancelled by user"}"#;
const ASKING: &str = r#"{"kind":"asking_opened"}"#;
const RESUMED: &str = r#"{"kind":"run_resumed"}"#;

/// A check after the work whose own criterion is red, and whose guard ran
/// for `guard_secs` anyway.
fn checked_red(guard_secs: u64) -> String {
    format!(
        r#"{{"kind":"criteria_checked","task_id":"T1","phase":"post","results":[{{"cmd":"test -f a","exit_code":1,"reused":false,"duration_ms":1000}},{{"cmd":"cargo test","exit_code":1,"type":"guard","reused":false,"duration_ms":{}}}]}}"#,
        guard_secs * 1000
    )
}

#[test]
fn every_stretch_of_a_run_is_the_time_of_one_thing() {
    let red = checked_red(80);
    let events = log(&[
        (0, None, CREATED),
        (600, None, MEASURED),
        (600, Some("a"), STARTED),
        (700, Some("a"), &red),
        (710, Some("a"), ASKED),
        (800, Some("a"), ANSWERED),
        (820, Some("a"), FINISHED),
        // A gate asked at the terminal reaches the log only once it is
        // answered — started, waiting and resolved in one instant — and
        // the engine says when it began asking.
        (820, Some("approve"), ASKING),
        (1120, Some("approve"), STARTED),
        (1120, Some("approve"), WAITING),
        (1120, Some("approve"), RESOLVED),
        (1120, Some("approve"), FINISHED),
        (1121, None, PAUSED),
        (2121, None, RESUMED),
    ]);

    let time = time_of(&events);

    assert_eq!(
        time,
        TimeSpent {
            measuring: Duration::from_secs(600),
            checks: Duration::from_secs(100),
            decided_checks: Duration::from_secs(80),
            working: Duration::from_secs(30),
            people: Duration::from_secs(90 + 300),
            parked: Duration::from_secs(1000),
            between: Duration::from_secs(1),
        }
    );
    assert_eq!(time.total(), Duration::from_secs(2121));
}

/// A guard before the work is asked whether the tree it starts from holds,
/// with the task's own criteria meant to be red: nothing it runs there was
/// decided.
#[test]
fn a_guard_before_the_work_is_never_counted_as_decided() {
    let pre = checked_red(80).replace(r#""phase":"post""#, r#""phase":"pre""#);
    let events = log(&[
        (0, None, CREATED),
        (0, Some("a"), STARTED),
        (100, Some("a"), &pre),
        (110, Some("a"), FINISHED),
    ]);

    let time = time_of(&events);

    assert_eq!(time.checks, Duration::from_secs(100));
    assert_eq!(time.decided_checks, Duration::ZERO);
    assert_eq!(time.working, Duration::from_secs(10));
}

/// A person's wait opens when the engine begins asking, and lasts until
/// the answer the asking ends with — though its events are written a
/// moment apart, one after another.
#[test]
fn a_person_s_wait_runs_from_the_asking_to_the_answer() {
    let events = log_ms(&[
        (0, None, CREATED),
        (0, Some("a"), STARTED),
        (10_000, Some("a"), FINISHED),
        (10_000, Some("approve"), ASKING),
        (310_000, Some("approve"), STARTED),
        (310_002, Some("approve"), WAITING),
        (310_004, Some("approve"), RESOLVED),
        (310_006, Some("approve"), FINISHED),
    ]);

    let time = time_of(&events);

    assert_eq!(time.people, Duration::from_millis(300_004));
    assert_eq!(time.between, Duration::ZERO);
}

/// An asking nobody answered ends with the invocation: what follows a
/// pause is parked, not a person's.
#[test]
fn a_person_s_wait_ends_when_the_run_pauses() {
    let events = log(&[
        (0, None, CREATED),
        (10, Some("approve"), ASKING),
        (40, None, PAUSED),
        (100, None, RESUMED),
    ]);

    let time = time_of(&events);

    assert_eq!(time.people, Duration::from_secs(30));
    assert_eq!(time.parked, Duration::from_secs(60));
    assert_eq!(time.between, Duration::from_secs(10));
}

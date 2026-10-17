//! What a duration the run reports leaves out: the spans its host slept.

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use yunta_core::events::{
    EventMeta, HostSuspendedPayload, RunEvent, RunLedger, Suspension, Suspensions,
};

fn at(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::minutes(minutes)
}

fn minutes(n: u64) -> Duration {
    Duration::from_secs(n * 60)
}

/// The spans a log recording `woke` (minute the host was noticed awake,
/// minutes it slept) folds into.
fn slept(woke: &[(i64, u64)]) -> Suspensions {
    let mut ledger = RunLedger::default();
    for (seq, (woke_at, slept_for)) in woke.iter().enumerate() {
        ledger.apply(
            &RunEvent::HostSuspended(HostSuspendedPayload::slept(minutes(*slept_for))),
            &EventMeta {
                seq: (seq as u64 + 1).into(),
                at: at(*woke_at),
                node: None,
            },
        );
    }
    ledger.suspensions().clone()
}

#[test]
fn a_suspension_inside_an_interval_is_left_out_of_it() {
    // Asleep from minute 10 to minute 70, measured from 0 to 100.
    let asleep = slept(&[(70, 60)]);
    assert_eq!(asleep.asleep_between(at(0), at(100)), minutes(60));
    assert_eq!(asleep.awake_between(at(0), at(100)), minutes(40));
}

#[test]
fn a_suspension_across_an_edge_is_left_out_only_where_they_overlap() {
    // Asleep from minute 10 to 70: measuring from 40 leaves out 30.
    let asleep = slept(&[(70, 60)]);
    assert_eq!(asleep.awake_between(at(40), at(100)), minutes(30));
    assert_eq!(asleep.awake_between(at(0), at(40)), minutes(10));
}

#[test]
fn an_interval_no_suspension_touches_is_its_wall_clock() {
    let asleep = slept(&[(70, 60)]);
    assert_eq!(asleep.awake_between(at(80), at(95)), minutes(15));
    assert_eq!(
        Suspensions::default().awake_between(at(0), at(5)),
        minutes(5)
    );
}

#[test]
fn every_suspension_is_summed_and_counted() {
    let asleep = slept(&[(70, 60), (130, 20)]);
    assert_eq!(asleep.awake_between(at(0), at(200)), minutes(120));
    assert_eq!(asleep.summary(), Some((2, minutes(80))));
    assert_eq!(
        asleep.iter().next(),
        Some(&Suspension {
            woke_at: at(70),
            slept: minutes(60),
        })
    );
}
